//! Helper programs Velora fetches on first use: yt-dlp (site and stream extraction) and ffmpeg
//! (merging separate video/audio streams, remuxing HLS).
//!
//! Each file is downloaded over HTTPS from the project's official GitHub release, checked against
//! the SHA-256 list published in the same release, and only then moved into place. Nothing is run
//! before it has been verified. yt-dlp is refreshed in the background once it is a week old, because
//! sites change and old versions stop working.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::model::ErrorInfo;

const YTDLP_BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const FFMPEG_BASE: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";
#[cfg(windows)]
const YTDLP_ASSET: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const YTDLP_ASSET: &str = "yt-dlp_linux";
/// LGPL build: everything needed to merge and remux, without GPL-only encoders.
const FFMPEG_ASSET: &str = "ffmpeg-master-latest-win64-lgpl.zip";
const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const MAX_YTDLP_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FFMPEG_BYTES: u64 = 512 * 1024 * 1024;
const YTDLP_REFRESH: Duration = Duration::from_secs(7 * 24 * 3600);
/// Give up on a helper download that sends nothing for this long.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Tools {
    /// `<app data>/tools`. `None` disables installing (tests); helpers on `PATH` are still used.
    dir: Option<PathBuf>,
    ytdlp_lock: Mutex<()>,
    ffmpeg_lock: Mutex<()>,
    refresh_started: AtomicBool,
    setup: Arc<StdMutex<Option<ToolSetup>>>,
}

/// A helper program being downloaded, for the UI ("Setting up video support… 40%").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSetup {
    /// `yt-dlp` or `ffmpeg`.
    pub tool: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Reports one install's progress into [`Tools::setup`] and clears it when dropped.
struct Progress {
    tool: &'static str,
    shared: Option<Arc<StdMutex<Option<ToolSetup>>>>,
}

impl Progress {
    fn silent() -> Self {
        Self { tool: "", shared: None }
    }

    fn update(&self, downloaded: u64, total: Option<u64>) {
        if let Some(shared) = &self.shared {
            *shared.lock().unwrap() = Some(ToolSetup { tool: self.tool.to_string(), downloaded, total });
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if let Some(shared) = &self.shared {
            *shared.lock().unwrap() = None;
        }
    }
}

fn tool_error(what: &str, detail: impl std::fmt::Display) -> ErrorInfo {
    ErrorInfo::new("tool_install", format!("Couldn't set up {what}: {detail}"))
}

impl Tools {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            ytdlp_lock: Mutex::new(()),
            ffmpeg_lock: Mutex::new(()),
            refresh_started: AtomicBool::new(false),
            setup: Arc::new(StdMutex::new(None)),
        }
    }

    /// The helper currently being installed, if any.
    pub fn setup(&self) -> Option<ToolSetup> {
        self.setup.lock().unwrap().clone()
    }

    fn progress(&self, tool: &'static str) -> Progress {
        let p = Progress { tool, shared: Some(self.setup.clone()) };
        p.update(0, None);
        p
    }

    fn installed_ytdlp(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("yt-dlp{EXE}")))
    }

    fn installed_ffmpeg_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("ffmpeg"))
    }

    /// yt-dlp if it is available without downloading anything.
    pub fn ytdlp(&self) -> Option<PathBuf> {
        self.installed_ytdlp().filter(|p| p.is_file()).or_else(|| on_path(&format!("yt-dlp{EXE}")))
    }

    /// The folder holding ffmpeg (and ffprobe), if available without downloading anything.
    pub fn ffmpeg_dir(&self) -> Option<PathBuf> {
        self.installed_ffmpeg_dir()
            .filter(|d| d.join(format!("ffmpeg{EXE}")).is_file())
            .or_else(|| on_path(&format!("ffmpeg{EXE}")).and_then(|p| p.parent().map(Path::to_path_buf)))
    }

    pub async fn ensure_ytdlp(&self, client: &Client) -> Result<PathBuf, ErrorInfo> {
        let _guard = self.ytdlp_lock.lock().await;
        if let Some(path) = self.ytdlp() {
            if self.installed_ytdlp().as_ref() == Some(&path) {
                self.maybe_refresh(client, &path);
            }
            return Ok(path);
        }
        let Some(target) = self.installed_ytdlp() else {
            return Err(ErrorInfo::new("tool_missing", "yt-dlp isn't installed, so Velora can't read this page."));
        };
        tracing::info!("installing yt-dlp");
        install_ytdlp(client, &target, &self.progress("yt-dlp")).await?;
        tracing::info!("yt-dlp installed");
        Ok(target)
    }

    pub async fn ensure_ffmpeg(&self, client: &Client) -> Result<PathBuf, ErrorInfo> {
        let _guard = self.ffmpeg_lock.lock().await;
        if let Some(dir) = self.ffmpeg_dir() {
            return Ok(dir);
        }
        let Some(dir) = self.installed_ffmpeg_dir().filter(|_| cfg!(windows)) else {
            return Err(ErrorInfo::new(
                "tool_missing",
                "ffmpeg is needed to join the video and audio streams. Install it and add it to PATH.",
            ));
        };
        tracing::info!("installing ffmpeg");
        install_ffmpeg(client, &dir, &self.progress("ffmpeg")).await?;
        tracing::info!("ffmpeg installed");
        Ok(dir)
    }

    /// Replace a week-old yt-dlp in the background. Failures are logged and otherwise ignored.
    fn maybe_refresh(&self, client: &Client, path: &Path) {
        let age = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
        if age.is_none_or(|a| a < YTDLP_REFRESH) || self.refresh_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let (client, path) = (client.clone(), path.to_path_buf());
        tokio::spawn(async move {
            match install_ytdlp(&client, &path, &Progress::silent()).await {
                Ok(()) => tracing::info!("yt-dlp updated"),
                Err(e) => tracing::warn!(error = %e.message, "yt-dlp update failed"),
            }
        });
    }
}

async fn install_ytdlp(client: &Client, target: &Path, progress: &Progress) -> Result<(), ErrorInfo> {
    let dir = target.parent().ok_or_else(|| tool_error("yt-dlp", "invalid folder"))?;
    std::fs::create_dir_all(dir).map_err(|e| tool_error("yt-dlp", e))?;
    // A background refresh and a first install never run at once, but keep their files apart anyway.
    let tmp = dir.join(format!("yt-dlp{EXE}.{}.download", std::process::id()));
    let expected = published_hash(client, &format!("{YTDLP_BASE}/SHA2-256SUMS"), YTDLP_ASSET).await.map_err(|e| tool_error("yt-dlp", e))?;
    let actual = fetch_to_file(client, &format!("{YTDLP_BASE}/{YTDLP_ASSET}"), &tmp, MAX_YTDLP_BYTES, progress)
        .await
        .map_err(|e| tool_error("yt-dlp", e))?;
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = std::fs::remove_file(&tmp);
        return Err(tool_error("yt-dlp", "the downloaded file didn't match its published checksum"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        tool_error("yt-dlp", e)
    })
}

async fn install_ffmpeg(client: &Client, dir: &Path, progress: &Progress) -> Result<(), ErrorInfo> {
    let parent = dir.parent().ok_or_else(|| tool_error("ffmpeg", "invalid folder"))?;
    std::fs::create_dir_all(parent).map_err(|e| tool_error("ffmpeg", e))?;
    let zip_path = parent.join("ffmpeg.zip.download");
    let expected =
        published_hash(client, &format!("{FFMPEG_BASE}/checksums.sha256"), FFMPEG_ASSET).await.map_err(|e| tool_error("ffmpeg", e))?;
    let actual = fetch_to_file(client, &format!("{FFMPEG_BASE}/{FFMPEG_ASSET}"), &zip_path, MAX_FFMPEG_BYTES, progress)
        .await
        .map_err(|e| tool_error("ffmpeg", e))?;
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = std::fs::remove_file(&zip_path);
        return Err(tool_error("ffmpeg", "the downloaded file didn't match its published checksum"));
    }
    let staging = parent.join("ffmpeg.staging");
    let result = {
        let (zip_path, staging) = (zip_path.clone(), staging.clone());
        tokio::task::spawn_blocking(move || extract_binaries(&zip_path, &staging, &["ffmpeg.exe", "ffprobe.exe"]))
            .await
            .map_err(|e| tool_error("ffmpeg", e))?
    };
    let _ = std::fs::remove_file(&zip_path);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(tool_error("ffmpeg", e));
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&staging, dir).map_err(|e| tool_error("ffmpeg", e))
}

/// Copy the named files from anywhere inside the archive (BtbN puts them in `<build>/bin/`).
fn extract_binaries(zip_path: &Path, out: &Path, names: &[&str]) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(std::io::Error::other)?;
    let mut found = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(std::io::Error::other)?;
        // Only the base name is used, and only for the expected programs, so archive paths never
        // decide where anything is written.
        let Ok(file_name) = entry.name().map(|n| n.rsplit('/').next().unwrap_or_default().to_ascii_lowercase()) else { continue };
        if entry.is_file() && names.contains(&file_name.as_str()) {
            let mut f = std::fs::File::create(out.join(&file_name))?;
            std::io::copy(&mut entry, &mut f)?;
            f.flush()?;
            found += 1;
        }
    }
    if found != names.len() {
        return Err(std::io::Error::other("the archive didn't contain the expected programs"));
    }
    Ok(())
}

/// Look up `asset` in a `sha256sum`-style list (`<hex>  <name>` per line).
async fn published_hash(client: &Client, url: &str, asset: &str) -> Result<String, String> {
    let fetch = async {
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("the checksum list returned HTTP {}", resp.status().as_u16()));
        }
        resp.text().await.map_err(|e| e.to_string())
    };
    let text = tokio::time::timeout(FETCH_POLICY.header_timeout * 2, fetch).await.map_err(|_| "GitHub didn't respond".to_string())??;
    parse_sums(&text, asset).ok_or_else(|| format!("{asset} isn't listed in the published checksums"))
}

pub(crate) fn parse_sums(text: &str, asset: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

/// When to give up on a connection and open a new one.
#[derive(Clone, Copy)]
struct FetchPolicy {
    /// Waiting for the server to answer a request.
    header_timeout: Duration,
    /// No data at all for this long.
    stall_timeout: Duration,
    /// A connection that delivers less than `min_window_bytes` in `slow_window` is replaced: a slow
    /// CDN edge is common, and a fresh connection usually lands on a faster one.
    slow_window: Duration,
    min_window_bytes: u64,
    max_reconnects: u32,
}

const MAX_UNANSWERED: u32 = 2;

const FETCH_POLICY: FetchPolicy = FetchPolicy {
    header_timeout: Duration::from_secs(30),
    stall_timeout: STALL_TIMEOUT,
    slow_window: Duration::from_secs(15),
    min_window_bytes: 15 * 64 * 1024,
    max_reconnects: 8,
};

/// Stream `url` into `path`, returning the SHA-256 of what was written. Slow or stalled connections
/// are replaced and the transfer continues from the current byte with a `Range` request.
async fn fetch_to_file(client: &Client, url: &str, path: &Path, max: u64, progress: &Progress) -> Result<String, String> {
    match fetch_with(client, url, path, max, progress, FETCH_POLICY).await {
        Ok(hash) => Ok(hash),
        Err(e) => {
            let _ = std::fs::remove_file(path);
            Err(e)
        }
    }
}

enum Interrupted {
    Slow,
    Stalled,
    Broken(String),
}

async fn fetch_with(client: &Client, url: &str, path: &Path, max: u64, progress: &Progress, policy: FetchPolicy) -> Result<String, String> {
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut reconnects = 0u32;
    // A server (or proxy) that doesn't answer at all is unlikely to start; fail fast instead of
    // keeping the user waiting.
    let mut unanswered = 0u32;
    loop {
        let mut req = client.get(url);
        if total > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={total}-"));
        }
        let resp = match tokio::time::timeout(policy.header_timeout, req.send()).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) if unanswered < MAX_UNANSWERED && (e.is_timeout() || e.is_connect() || e.is_request()) => {
                unanswered += 1;
                tracing::debug!(error = %e, unanswered, "helper download: retrying");
                continue;
            }
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) if unanswered < MAX_UNANSWERED => {
                unanswered += 1;
                tracing::debug!(unanswered, "helper download: no response, retrying");
                continue;
            }
            Err(_) => return Err("GitHub didn't respond. Check the internet connection or proxy settings".into()),
        };
        let status = resp.status();
        let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if total > 0 && !resumed {
            if !status.is_success() {
                return Err(format!("the download returned HTTP {}", status.as_u16()));
            }
            // The server ignored the range: start over with this response.
            file.set_len(0).map_err(|e| e.to_string())?;
            std::io::Seek::rewind(&mut file).map_err(|e| e.to_string())?;
            hasher = Sha256::new();
            total = 0;
        } else if !status.is_success() {
            return Err(format!("the download returned HTTP {}", status.as_u16()));
        }
        if resumed {
            let start = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(crate::net::range::parse_content_range)
                .map(|r| r.start);
            if start != Some(total) {
                return Err("the server sent the wrong part of the file".into());
            }
        }
        let expected = resp.content_length().map(|n| n + total);
        if expected.is_some_and(|n| n > max) {
            return Err("the file is unexpectedly large".into());
        }
        progress.update(total, expected);

        let mut stream = resp.bytes_stream();
        let mut window_start = tokio::time::Instant::now();
        let mut window_bytes = 0u64;
        let interrupted = loop {
            let chunk = match tokio::time::timeout(policy.stall_timeout, stream.next()).await {
                Err(_) => break Some(Interrupted::Stalled),
                Ok(None) => break None,
                Ok(Some(Err(e))) => break Some(Interrupted::Broken(e.to_string())),
                Ok(Some(Ok(chunk))) => chunk,
            };
            total += chunk.len() as u64;
            if total > max {
                return Err("the file is unexpectedly large".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            progress.update(total, expected);
            window_bytes += chunk.len() as u64;
            if window_start.elapsed() >= policy.slow_window {
                if window_bytes < policy.min_window_bytes && reconnects < policy.max_reconnects {
                    break Some(Interrupted::Slow);
                }
                window_start = tokio::time::Instant::now();
                window_bytes = 0;
            }
        };
        let interrupted = match interrupted {
            None if expected.is_some_and(|n| total < n) => Interrupted::Broken("the connection closed early".into()),
            None => break,
            Some(i) => i,
        };
        if reconnects >= policy.max_reconnects {
            return Err(match interrupted {
                Interrupted::Stalled => "the download stopped sending data".into(),
                Interrupted::Slow => "the download is too slow".into(),
                Interrupted::Broken(e) => e,
            });
        }
        reconnects += 1;
        tracing::debug!(total, reconnects, slow = matches!(interrupted, Interrupted::Slow), "helper download: reconnecting");
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_lookup() {
        let text = "aa  other\n33121d192d706e62f684a06f80918e3f3a8ba52ac3a65213b6951c39c7d9f555  yt-dlp.exe\n";
        assert_eq!(parse_sums(text, "yt-dlp.exe").as_deref(), Some("33121d192d706e62f684a06f80918e3f3a8ba52ac3a65213b6951c39c7d9f555"));
        assert_eq!(parse_sums(text, "yt-dlp"), None);
        assert_eq!(parse_sums("abc  *x.zip", "x.zip"), None);
    }

    /// The first connection sends a little and then trickles; the download must switch to a new
    /// connection, resume with `Range`, and still produce the right bytes and hash.
    #[tokio::test]
    async fn slow_connection_is_replaced_and_resumed() {
        use axum::body::Body;
        use axum::http::{header, HeaderMap, StatusCode};
        use axum::response::Response;
        use std::sync::atomic::AtomicUsize;

        let data: Arc<Vec<u8>> = Arc::new((0..3 * 1024 * 1024u32).map(|i| (i * 7 % 251) as u8).collect());
        let requests = Arc::new(AtomicUsize::new(0));
        let app = {
            let (data, requests) = (data.clone(), requests.clone());
            axum::Router::new().route(
                "/tool",
                axum::routing::get(move |headers: HeaderMap| {
                    let (data, requests) = (data.clone(), requests.clone());
                    async move {
                        let n = requests.fetch_add(1, Ordering::SeqCst);
                        let start = headers
                            .get(header::RANGE)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.strip_prefix("bytes="))
                            .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok())
                            .unwrap_or(0);
                        let len = data.len();
                        let body = data[start..].to_vec();
                        let stream: futures_util::stream::BoxStream<'static, Result<Vec<u8>, std::io::Error>> = if n == 0 {
                            // 64 KiB quickly, then 1 KiB every 100 ms.
                            let (head, tail) = body.split_at(64 * 1024);
                            let tail: Vec<Vec<u8>> = tail.chunks(1024).map(<[u8]>::to_vec).collect();
                            let head = futures_util::stream::once(futures_util::future::ready(Ok(head.to_vec())));
                            let slow = futures_util::stream::iter(tail).then(|c| async move {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                Ok(c)
                            });
                            Box::pin(head.chain(slow))
                        } else {
                            Box::pin(futures_util::stream::iter(body.chunks(64 * 1024).map(|c| Ok(c.to_vec())).collect::<Vec<_>>()))
                        };
                        let mut resp =
                            Response::builder().header(header::CONTENT_LENGTH, len - start).header(header::ACCEPT_RANGES, "bytes");
                        if start > 0 {
                            resp = resp
                                .status(StatusCode::PARTIAL_CONTENT)
                                .header(header::CONTENT_RANGE, format!("bytes {start}-{}/{len}", len - 1));
                        }
                        resp.body(Body::from_stream(stream)).unwrap()
                    }
                }),
            )
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool.download");
        let policy = FetchPolicy {
            header_timeout: Duration::from_secs(5),
            stall_timeout: Duration::from_secs(5),
            slow_window: Duration::from_millis(500),
            min_window_bytes: 512 * 1024,
            max_reconnects: 3,
        };
        let shared = Arc::new(StdMutex::new(None));
        let progress = Progress { tool: "test", shared: Some(shared.clone()) };
        let hash = fetch_with(&Client::new(), &format!("http://{addr}/tool"), &path, 64 * 1024 * 1024, &progress, policy).await.unwrap();

        let expected: String = Sha256::digest(&*data).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash, expected);
        assert_eq!(std::fs::read(&path).unwrap(), *data);
        assert!(requests.load(Ordering::SeqCst) >= 2, "a second connection was opened");
        assert_eq!(shared.lock().unwrap().as_ref().map(|s| (s.downloaded, s.total)), Some((data.len() as u64, Some(data.len() as u64))));
        drop(progress);
        assert!(shared.lock().unwrap().is_none(), "progress clears when the install ends");
    }
}
