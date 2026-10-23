//! BitTorrent downloads: magnet links and `.torrent` URLs, through librqbit.
//!
//! One session is shared by all torrents and started on first use. Velora only downloads: a torrent
//! is removed from the session (its files kept) when it finishes, is paused, or is cancelled, so
//! nothing keeps running in the background. Resuming adds it again and librqbit re-checks the pieces
//! already on disk.
//!
//! Torrent metadata is cached as `<info hash>.torrent` in the app's data folder, so a magnet link
//! only has to be resolved through the network once.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use librqbit::api::TorrentIdOrHash;
use librqbit::limits::LimitsConfig;
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, Magnet, Session, SessionOptions, TorrentStatsState};
use tokio::sync::OnceCell;
use url::Url;

use crate::category::categorize;
use crate::download::task::{TaskEnv, TaskOutcome};
use crate::model::{Category, DownloadRecord, DownloadStatus, ErrorInfo, ProbeInfo};
use crate::security::{confine, sanitize_file_name, unique_file_name, validate_dest_dir};
use crate::util::now_ms;

pub const TORRENT_MIME: &str = "application/x-bittorrent";
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(120);
const TICK: Duration = Duration::from_millis(500);
const MAX_TORRENT_FILE: usize = 16 * 1024 * 1024;

/// A magnet link, or an http(s) link to a `.torrent` file.
pub fn is_torrent_link(input: &str) -> bool {
    let t = input.trim();
    if t.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("magnet:")) {
        return true;
    }
    Url::parse(t).is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.path().to_ascii_lowercase().ends_with(".torrent"))
}

/// Validate a torrent link and return it in canonical form.
pub fn parse_torrent_link(input: &str) -> Result<String, ErrorInfo> {
    let t = input.trim();
    if t.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("magnet:")) {
        let magnet = Magnet::parse(t).map_err(|_| ErrorInfo::new("invalid_url", "This magnet link isn't valid."))?;
        if magnet.as_id20().is_none() {
            return Err(ErrorInfo::new("invalid_url", "This magnet link uses BitTorrent v2 only, which Velora doesn't support yet."));
        }
        return Ok(t.to_string());
    }
    crate::security::parse_download_url(t).map(|u| u.to_string())
}

/// A file name to show before the metadata is known.
pub fn name_hint(link: &str) -> String {
    if let Ok(m) = Magnet::parse(link.trim()) {
        if let Some(name) = m.name.as_deref().filter(|n| !n.trim().is_empty()) {
            return sanitize_file_name(name);
        }
        if let Some(id) = m.as_id20() {
            return format!("torrent-{}", &id.as_string()[..8]);
        }
    }
    let stem = Url::parse(link.trim())
        .ok()
        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back().map(str::to_string)))
        .map(|last| last.trim_end_matches(".torrent").to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "torrent".into());
    sanitize_file_name(&percent_encoding::percent_decode_str(&stem).decode_utf8_lossy())
}

/// What a torrent contains.
pub struct TorrentMeta {
    pub info_hash: String,
    pub name: String,
    /// Path inside the torrent (`/`-separated) and length of each file.
    pub files: Vec<(String, u64)>,
    pub total: u64,
    bytes: Bytes,
}

impl TorrentMeta {
    pub fn category(&self) -> Category {
        match self.files.iter().max_by_key(|(_, len)| *len) {
            Some((path, _)) => categorize(path, None),
            None => Category::Other,
        }
    }

    fn single_file(&self) -> Option<&str> {
        match self.files.as_slice() {
            [(path, _)] if !path.contains('/') => Some(path),
            _ => None,
        }
    }

    /// The name to show for a torrent: the file for single-file torrents, the folder otherwise.
    pub fn display_name(&self) -> String {
        sanitize_file_name(self.single_file().unwrap_or(&self.name))
    }
}

pub struct TorrentHub {
    /// `<app data>/torrents`: metadata cache and DHT state. `None` keeps nothing on disk.
    dir: Option<PathBuf>,
    default_dir: PathBuf,
    session: OnceCell<Arc<Session>>,
    meta: Mutex<HashMap<String, Arc<TorrentMeta>>>,
}

fn torrent_error(e: impl std::fmt::Display) -> ErrorInfo {
    ErrorInfo::new("torrent", format!("BitTorrent error: {e}"))
}

impl TorrentHub {
    pub fn new(dir: Option<PathBuf>, default_dir: PathBuf) -> Self {
        Self { dir, default_dir, session: OnceCell::new(), meta: Mutex::new(HashMap::new()) }
    }

    pub async fn session(&self) -> Result<Arc<Session>, ErrorInfo> {
        self.session
            .get_or_try_init(|| async {
                if let Some(dir) = &self.dir {
                    std::fs::create_dir_all(dir).map_err(torrent_error)?;
                }
                let dht = DhtSessionConfig {
                    persistence: self
                        .dir
                        .as_ref()
                        .map(|d| librqbit::dht::DhtPersistenceConfig { dump_interval: None, config_filename: Some(d.join("dht.json")) }),
                    ..Default::default()
                };
                let opts = SessionOptions {
                    dht: Some(dht),
                    // Outgoing connections only: no listening socket, so no firewall prompt.
                    listen: None,
                    persistence: None,
                    fastresume: false,
                    client_name_and_version: Some(format!("Velora {}", env!("CARGO_PKG_VERSION"))),
                    ..Default::default()
                };
                Session::new_with_opts(self.default_dir.clone(), opts).await.map_err(torrent_error)
            })
            .await
            .cloned()
    }

    fn cache_path(&self, info_hash: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{info_hash}.torrent")))
    }

    /// Fetch (or recall) what `link` contains. Magnet links are resolved through DHT and trackers.
    pub async fn metadata(&self, link: &str) -> Result<Arc<TorrentMeta>, ErrorInfo> {
        if let Some(m) = self.meta.lock().unwrap().get(link) {
            return Ok(m.clone());
        }
        let cached = Magnet::parse(link.trim())
            .ok()
            .and_then(|m| m.as_id20())
            .and_then(|id| self.cache_path(&id.as_string()))
            .and_then(|p| std::fs::read(p).ok())
            .filter(|b| b.len() <= MAX_TORRENT_FILE);
        let session = self.session().await?;
        let add = match cached {
            Some(bytes) => AddTorrent::from_bytes(bytes),
            None => AddTorrent::from_url(link.trim().to_string()),
        };
        let opts = AddTorrentOptions { list_only: true, ..Default::default() };
        let resp = tokio::time::timeout(RESOLVE_TIMEOUT, session.add_torrent(add, Some(opts)))
            .await
            .map_err(|_| ErrorInfo::new("timeout", "No peers answered with this torrent's details. Check the link or try again later."))?
            .map_err(torrent_error)?;
        let AddTorrentResponse::ListOnly(list) = resp else {
            return Err(torrent_error("unexpected response while reading the torrent"));
        };
        let files: Vec<(String, u64)> = list.info.iter_file_details().map(|f| (f.filename.to_vec().join("/"), f.len)).collect();
        let meta = Arc::new(TorrentMeta {
            info_hash: list.info_hash.as_string(),
            name: list.info.name().map(|n| n.into_owned()).unwrap_or_else(|| name_hint(link)),
            total: files.iter().map(|(_, l)| l).sum(),
            files,
            bytes: list.torrent_bytes,
        });
        if let Some(path) = self.cache_path(&meta.info_hash) {
            if !path.exists() {
                if let Err(e) = std::fs::write(&path, &meta.bytes) {
                    tracing::warn!(error = %e, "couldn't cache torrent metadata");
                }
            }
        }
        self.meta.lock().unwrap().insert(link.to_string(), meta.clone());
        Ok(meta)
    }

    pub async fn probe(&self, link: &str) -> Result<ProbeInfo, ErrorInfo> {
        let meta = self.metadata(link).await?;
        let count = meta.files.len();
        Ok(ProbeInfo {
            url: link.to_string(),
            final_url: link.to_string(),
            file_name: meta.display_name(),
            host: "BitTorrent".into(),
            size: Some(meta.total),
            mime: Some(format!("{count} file{}", if count == 1 { "" } else { "s" })),
            resumable: true,
            etag: None,
            last_modified: None,
            content_encoding: None,
            category: meta.category(),
        })
    }
}

/// Where a torrent's data goes: `(name shown in the list, folder handed to librqbit)`.
///
/// Multi-file torrents get their own folder. A single file goes straight into the destination
/// unless that name is taken, in which case it gets a folder of its own; renaming the file itself
/// isn't possible because the name is part of the torrent.
fn layout(meta: &TorrentMeta, dest: &Path, taken: &dyn Fn(&str) -> bool) -> (String, PathBuf) {
    if let Some(file) = meta.single_file() {
        let clean = sanitize_file_name(file);
        if clean == file && !dest.join(&clean).exists() && !taken(&clean) {
            return (clean, dest.to_path_buf());
        }
        let stem = clean.rsplit_once('.').map_or(clean.as_str(), |(s, _)| s).to_string();
        let folder = unique_file_name(dest, &stem, taken);
        return (folder.clone(), dest.join(folder));
    }
    let folder = unique_file_name(dest, &sanitize_file_name(&meta.name), taken);
    (folder.clone(), dest.join(folder))
}

/// The folder librqbit writes to for an already laid-out torrent.
fn output_folder(meta: &TorrentMeta, rec: &DownloadRecord) -> PathBuf {
    let dest = Path::new(&rec.dest_dir);
    match meta.single_file() {
        Some(file) if file == rec.file_name => dest.to_path_buf(),
        _ => dest.join(&rec.file_name),
    }
}

/// Delete the data of an unfinished torrent. Only paths Velora laid out itself (recorded by the info
/// hash) are touched; those were chosen so they didn't exist before.
pub fn delete_partial(rec: &DownloadRecord) {
    if rec.info_hash.is_none() || rec.status == DownloadStatus::Completed {
        return;
    }
    let target = rec.target_path();
    let res = if target.is_dir() {
        std::fs::remove_dir_all(&target)
    } else if target.is_file() {
        std::fs::remove_file(&target)
    } else {
        Ok(())
    };
    if let Err(e) = res {
        tracing::warn!(path = %target.display(), error = %e, "couldn't delete partial torrent data");
    }
}

pub async fn run(env: TaskEnv, hub: Arc<TorrentHub>) -> TaskOutcome {
    match run_inner(&env, &hub).await {
        Ok(()) => TaskOutcome::Completed,
        Err(None) => TaskOutcome::Cancelled,
        Err(Some(info)) => TaskOutcome::Failed(info),
    }
}

async fn cancellable<T>(env: &TaskEnv, fut: impl std::future::Future<Output = Result<T, ErrorInfo>>) -> Result<T, Option<ErrorInfo>> {
    tokio::select! {
        _ = env.cancel.cancelled() => Err(None),
        r = fut => r.map_err(Some),
    }
}

/// `Err(None)` is a cancellation.
async fn run_inner(env: &TaskEnv, hub: &TorrentHub) -> Result<(), Option<ErrorInfo>> {
    let rec = env.hooks.record(&env.id).ok_or_else(|| Some(ErrorInfo::new("internal", "The download record is missing.")))?;
    let session = cancellable(env, hub.session()).await?;
    let meta = cancellable(env, hub.metadata(&rec.url)).await?;

    // Lay out the files the first time; afterwards the record says where they are.
    let mut rec = rec;
    if rec.info_hash.is_none() {
        let category = meta.category();
        let dest_dir = if rec.dest_auto { env.hooks.dir_for_category(category) } else { rec.dest_dir.clone() };
        let dest = validate_dest_dir(&dest_dir).map_err(Some)?;
        tokio::fs::create_dir_all(&dest)
            .await
            .map_err(|e| Some(ErrorInfo::new("io", format!("Couldn't create the download folder: {e}"))))?;
        let id = env.id.clone();
        let (name, _) = layout(&meta, &dest, &|n| env.hooks.path_reserved(&id, &dest.join(n)));
        confine(&dest, &name).map_err(Some)?;
        env.hooks.update(&env.id, &mut |r| {
            r.file_name = name.clone();
            r.dest_dir = dest_dir.clone();
            r.category = category;
            r.name_auto = false;
            r.info_hash = Some(meta.info_hash.clone());
        });
        rec = env.hooks.record(&env.id).ok_or_else(|| Some(ErrorInfo::new("internal", "The download record is missing.")))?;
    }
    let output = output_folder(&meta, &rec);
    tokio::fs::create_dir_all(&output)
        .await
        .map_err(|e| Some(ErrorInfo::new("io", format!("Couldn't create the download folder: {e}"))))?;
    env.hooks.update(&env.id, &mut |r| {
        r.status = DownloadStatus::Downloading;
        r.total_size = Some(meta.total);
        r.mime = Some(TORRENT_MIME.into());
        r.resumable = true;
        r.restart_pending = false;
        r.error = None;
        r.started_at.get_or_insert_with(now_ms);
    });

    let opts = AddTorrentOptions {
        output_folder: Some(output.to_string_lossy().into_owned()),
        // Needed to continue from files already on disk; the folder is Velora's own.
        overwrite: true,
        ratelimits: LimitsConfig {
            download_bps: rec.speed_limit.and_then(|l| NonZeroU32::new(l.min(u32::MAX as u64) as u32)),
            upload_bps: None,
        },
        ..Default::default()
    };
    let resp = cancellable(env, async {
        session.add_torrent(AddTorrent::from_bytes(meta.bytes.clone()), Some(opts)).await.map_err(torrent_error)
    })
    .await?;
    let already = matches!(resp, AddTorrentResponse::AlreadyManaged(..));
    let handle = resp.into_handle().ok_or_else(|| Some(torrent_error("the torrent couldn't be started")))?;
    if already {
        let _ = session.unpause(&handle).await;
    }

    env.live.set_single(true);
    env.live.set_total(Some(meta.total));
    let mut last = None::<u64>;
    let mut tick = tokio::time::interval(TICK);
    let outcome = loop {
        tokio::select! {
            _ = env.cancel.cancelled() => break Err(None),
            _ = tick.tick() => {}
        }
        let st = handle.stats();
        if let Some(e) = &st.error {
            break Err(Some(torrent_error(e)));
        }
        let done = st.progress_bytes;
        // Pieces found on disk while checking aren't network traffic.
        if matches!(st.state, TorrentStatsState::Live) {
            env.live.add_received(done.saturating_sub(last.unwrap_or(done)));
        }
        last = Some(done);
        env.live.set_total(Some(st.total_bytes));
        env.live.set_single_bytes(done);
        let peers = st
            .live
            .as_ref()
            .and_then(|l| serde_json::to_value(&l.snapshot.peer_stats).ok())
            .and_then(|v| v.get("live").and_then(serde_json::Value::as_u64))
            .unwrap_or(0);
        env.live.set_connections(peers as u32);
        env.hooks.set_progress(&env.id, done);
        if st.finished {
            break Ok(st.total_bytes);
        }
    };
    env.live.set_connections(0);
    if let Err(e) = session.delete(TorrentIdOrHash::Id(handle.id()), false).await {
        tracing::warn!(id = %env.id, error = %e, "couldn't remove torrent from the session");
    }
    let total = outcome?;
    env.hooks.update(&env.id, &mut |r| {
        r.downloaded = total;
        r.total_size = Some(total);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGNET: &str =
        "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Fexplodie.org%3A6969";

    #[test]
    fn detects_links() {
        assert!(is_torrent_link(MAGNET));
        assert!(is_torrent_link("https://example.com/files/linux.iso.torrent?x=1"));
        assert!(!is_torrent_link("https://example.com/file.zip"));
        assert!(parse_torrent_link(MAGNET).is_ok());
        assert!(parse_torrent_link("magnet:?xt=urn:btih:nothex").is_err());
        assert_eq!(name_hint(MAGNET), "Big Buck Bunny");
        assert_eq!(name_hint("https://example.com/a/ubuntu%2024.torrent"), "ubuntu 24");
    }

    fn meta(files: &[(&str, u64)]) -> TorrentMeta {
        TorrentMeta {
            info_hash: "00".repeat(20),
            name: "Pack".into(),
            files: files.iter().map(|(p, l)| (p.to_string(), *l)).collect(),
            total: files.iter().map(|(_, l)| l).sum(),
            bytes: Bytes::new(),
        }
    }

    #[test]
    fn layout_places_files() {
        let dir = std::env::temp_dir().join(format!("velora-torrent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let none = |_: &str| false;

        let single = meta(&[("movie.mkv", 10)]);
        assert_eq!(layout(&single, &dir, &none), ("movie.mkv".to_string(), dir.clone()));
        std::fs::write(dir.join("movie.mkv"), b"x").unwrap();
        assert_eq!(layout(&single, &dir, &none), ("movie".to_string(), dir.join("movie")));

        let multi = meta(&[("a/b.txt", 1), ("c.mkv", 5)]);
        assert_eq!(layout(&multi, &dir, &none).0, "Pack");
        assert_eq!(multi.category(), Category::Video);
        assert_eq!(multi.display_name(), "Pack");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
