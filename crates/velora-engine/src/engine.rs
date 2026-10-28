//! The engine: owns records, settings, the queue and running tasks.
//!
//! Lock order (never acquire in the opposite direction): `running` → `records`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, Timelike};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::category::categorize;
use crate::download::checkpoint;
use crate::download::live::LiveState;
use crate::download::task::{self, sha256_file, TaskEnv, TaskHooks, TaskOutcome};
use crate::download::WriteFault;
use crate::media::ytdlp;
use crate::model::*;
use crate::net::probe::name_from_url;
use crate::net::{build_client, probe, RateLimiter};
use crate::security::{confine, parse_download_url, redact_url, sanitize_file_name, unique_file_name, validate_dest_dir};
use crate::settings::{Settings, MAX_CONNECTIONS};
use crate::storage::Store;
use crate::tools::Tools;
use crate::torrent::{self, TorrentHub};
use crate::util::{now_ms, same_path};

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct EngineConfig {
    /// SQLite database file. `None` keeps everything in memory (tests).
    pub db_path: Option<PathBuf>,
    /// App data folder for helper programs (`tools/`) and torrent metadata (`torrents/`). `None`
    /// disables installing helpers; programs already on `PATH` are still used.
    pub data_dir: Option<PathBuf>,
    /// Used when the stored settings have no download folder yet.
    pub default_download_dir: PathBuf,
    pub on_event: EventSink,
    /// Scheduler / progress tick.
    pub tick: Duration,
    /// Test hook for simulated disk failures.
    pub write_fault: Option<WriteFault>,
}

impl EngineConfig {
    pub fn new(db_path: Option<PathBuf>, default_download_dir: PathBuf, on_event: EventSink) -> Self {
        Self { db_path, data_dir: None, default_download_dir, on_event, tick: Duration::from_millis(500), write_fault: None }
    }

    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        self.data_dir = Some(dir);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClearScope {
    Completed,
    Failed,
    Finished,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum StopReason {
    None = 0,
    Pause = 1,
    Cancel = 2,
    Remove = 3,
    Shutdown = 4,
    QueueWindow = 5,
    Restart = 6,
}

impl StopReason {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Pause,
            2 => Self::Cancel,
            3 => Self::Remove,
            4 => Self::Shutdown,
            5 => Self::QueueWindow,
            6 => Self::Restart,
            _ => Self::None,
        }
    }
}

struct Running {
    cancel: CancellationToken,
    reason: Arc<AtomicU8>,
    live: Arc<LiveState>,
    own_limit: Arc<RateLimiter>,
    /// Started automatically by the queue (as opposed to by the user).
    auto: bool,
    handle: Option<JoinHandle<()>>,
}

impl Running {
    fn stop(&self, reason: StopReason) {
        // Keep the first reason: a pause followed by a shutdown is still a pause.
        let _ = self.reason.compare_exchange(StopReason::None as u8, reason as u8, Ordering::AcqRel, Ordering::Acquire);
        self.cancel.cancel();
    }
}

struct Inner {
    store: Store,
    records: Mutex<HashMap<String, DownloadRecord>>,
    running: Mutex<HashMap<String, Running>>,
    settings: RwLock<Settings>,
    client: RwLock<Client>,
    global_limit: Arc<RateLimiter>,
    on_event: EventSink,
    queue_paused: AtomicBool,
    speed_history: Mutex<VecDeque<u64>>,
    wake: Notify,
    shutdown: CancellationToken,
    write_fault: Option<WriteFault>,
    tools: Arc<Tools>,
    torrents: Arc<TorrentHub>,
    /// Engine methods may be called from threads outside the runtime (e.g. the UI thread).
    rt: tokio::runtime::Handle,
}

/// Cheap to clone handle to the engine.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

const GLOBAL_HISTORY: usize = 120;

impl Engine {
    /// Open the database, recover downloads interrupted by a crash or shutdown, and start the
    /// scheduler. Must be called inside a Tokio runtime.
    pub async fn open(config: EngineConfig) -> Result<Engine, ErrorInfo> {
        let store = match &config.db_path {
            Some(p) => Store::open(p)?,
            None => Store::open_in_memory()?,
        };
        let mut settings = store.load_settings()?.unwrap_or_default().normalized();
        if settings.default_dir.trim().is_empty() {
            settings.default_dir = config.default_download_dir.to_string_lossy().into_owned();
            store.save_settings(&settings)?;
        }
        let client = build_client(&settings).or_else(|e| {
            tracing::warn!(error = %e.message, "falling back to default network settings");
            build_client(&Settings::default())
        })?;

        let mut records = HashMap::new();
        let mut recovered = Vec::new();
        for mut rec in store.load_downloads()? {
            if rec.status.is_running() {
                rec.status = if settings.resume_on_startup { DownloadStatus::Queued } else { DownloadStatus::Paused };
                recovered.push(rec.id.clone());
            }
            if !rec.status.is_finished() && rec.resumable {
                // The checkpoint is authoritative for how much is safely on disk.
                let ck = checkpoint::checkpoint_path(&rec.target_path());
                rec.downloaded = checkpoint::load(&ck).filter(|c| c.id == rec.id).map_or(0, |c| c.written());
            }
            records.insert(rec.id.clone(), rec);
        }

        let queue_paused = store.load_value("queue_paused")?.as_deref() == Some("true");
        let inner = Arc::new(Inner {
            store,
            global_limit: Arc::new(RateLimiter::new(settings.global_speed_limit)),
            settings: RwLock::new(settings),
            client: RwLock::new(client),
            records: Mutex::new(records),
            running: Mutex::new(HashMap::new()),
            on_event: config.on_event,
            queue_paused: AtomicBool::new(queue_paused),
            speed_history: Mutex::new(VecDeque::with_capacity(GLOBAL_HISTORY)),
            wake: Notify::new(),
            shutdown: CancellationToken::new(),
            write_fault: config.write_fault,
            tools: Arc::new(Tools::new(config.data_dir.as_ref().map(|d| d.join("tools")))),
            torrents: Arc::new(TorrentHub::new(config.data_dir.as_ref().map(|d| d.join("torrents")), config.default_download_dir.clone())),
            rt: tokio::runtime::Handle::current(),
        });
        if !recovered.is_empty() {
            let recs: Vec<_> = {
                let map = inner.records.lock().unwrap();
                recovered.iter().filter_map(|id| map.get(id).cloned()).collect()
            };
            inner.store.upsert_many(&recs)?;
            tracing::info!(count = recs.len(), "recovered interrupted downloads");
        }

        inner.rt.spawn(scheduler_loop(inner.clone(), config.tick));
        Ok(Engine { inner })
    }

    // -- Queries ---------------------------------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.inner.settings.read().unwrap().clone()
    }

    pub fn list(&self) -> Vec<DownloadRecord> {
        let mut v: Vec<_> = self.inner.records.lock().unwrap().values().cloned().collect();
        v.sort_by_key(|r| (r.created_at, r.queue_pos));
        v
    }

    pub fn get(&self, id: &str) -> Option<DownloadRecord> {
        self.inner.records.lock().unwrap().get(id).cloned()
    }

    pub fn details(&self, id: &str) -> Option<DownloadDetails> {
        let (progress, segments, history) = {
            let running = self.inner.running.lock().unwrap();
            match running.get(id) {
                Some(r) => (Some(r.live.sample_peek(id)), r.live.segments(), r.live.speed_history()),
                None => (None, Vec::new(), Vec::new()),
            }
        };
        let record = self.get(id)?;
        let part_path = checkpoint::part_path(&record.target_path());
        let segments = if segments.is_empty() && !record.status.is_finished() {
            // Not running: show what the checkpoint says is on disk.
            checkpoint::load(&checkpoint::checkpoint_path(&record.target_path()))
                .filter(|c| c.id == record.id)
                .map(|c| {
                    c.segments.iter().map(|s| SegmentView { start: s.start, end: s.end, downloaded: s.written, active: false }).collect()
                })
                .unwrap_or_default()
        } else {
            segments
        };
        Some(DownloadDetails { record, progress, segments, speed_history: history, part_path: part_path.to_string_lossy().into_owned() })
    }

    pub fn stats(&self) -> Stats {
        let today_start = Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|d| d.and_local_timezone(Local).single())
            .map_or(0, |d| d.timestamp_millis());
        let mut s = Stats::default();
        {
            let records = self.inner.records.lock().unwrap();
            for r in records.values() {
                match r.status {
                    DownloadStatus::Queued => s.queued += 1,
                    DownloadStatus::Completed => {
                        let size = r.total_size.unwrap_or(r.downloaded);
                        s.total_completed += 1;
                        s.total_downloaded += size;
                        if r.completed_at.unwrap_or(0) >= today_start {
                            s.completed_today += 1;
                            s.downloaded_today += size;
                        }
                    }
                    _ => {}
                }
            }
        }
        s.active = self.inner.running.lock().unwrap().len() as u32;
        let history: Vec<u64> = self.inner.speed_history.lock().unwrap().iter().copied().collect();
        s.current_speed = history.last().copied().unwrap_or(0);
        let recent: Vec<u64> = history.iter().rev().take(60).copied().filter(|v| *v > 0).collect();
        s.average_speed = if recent.is_empty() { 0 } else { recent.iter().sum::<u64>() / recent.len() as u64 };
        s.speed_history = history;
        s.queue_paused = self.inner.queue_paused.load(Ordering::Acquire);
        s.effective_speed_limit = self.inner.global_limit.rate();
        s
    }

    // -- Settings --------------------------------------------------------------------------------

    pub fn update_settings(&self, new: Settings) -> Result<Settings, ErrorInfo> {
        let new = new.normalized();
        if !new.default_dir.trim().is_empty() {
            validate_dest_dir(&new.default_dir)?;
        }
        for dir in new.category_folders.values().filter(|d| !d.trim().is_empty()) {
            validate_dest_dir(dir)?;
        }
        let old = self.settings();
        let network_changed = old.proxy != new.proxy
            || old.connect_timeout_secs != new.connect_timeout_secs
            || old.read_timeout_secs != new.read_timeout_secs;
        if network_changed {
            let client = build_client(&new)?;
            *self.inner.client.write().unwrap() = client;
        }
        self.inner.store.save_settings(&new)?;
        *self.inner.settings.write().unwrap() = new.clone();
        self.inner.apply_speed_limit();
        self.inner.wake.notify_one();
        Ok(new)
    }

    // -- Adding ----------------------------------------------------------------------------------

    /// Ask the server about a URL (no retries, bounded by the read timeout).
    /// Ask the server about a URL (no retries, bounded by the read timeout). For torrents this reads
    /// the torrent's metadata, which for magnet links can take a while.
    pub async fn probe(&self, url: &str, referrer: Option<&str>) -> Result<ProbeInfo, ErrorInfo> {
        if torrent::is_torrent_link(url) {
            let link = torrent::parse_torrent_link(url)?;
            return self.inner.torrents.probe(&link).await;
        }
        let url = parse_download_url(url)?;
        let client = self.inner.client.read().unwrap().clone();
        let timeout = Duration::from_secs(self.settings().read_timeout_secs);
        match tokio::time::timeout(timeout, probe(&client, &url, referrer)).await {
            Ok(r) => r.map_err(|f| f.into_info()),
            Err(_) => Err(ErrorInfo::new("timeout", "The server took too long to respond.")),
        }
    }

    /// If `url` is a page or stream a media resolver understands (an X post, a video site, an HLS
    /// playlist…), return what it offers. `Ok(None)` means the URL should be downloaded as-is.
    ///
    /// Known video sites and stream manifests go to yt-dlp, which is installed on first use. Other
    /// web pages are offered to yt-dlp only once it is installed, so plain file links never wait
    /// for it.
    pub async fn resolve_media(&self, url: &str) -> Result<Option<crate::media::ResolvedMedia>, ErrorInfo> {
        if torrent::is_torrent_link(url) {
            return Ok(None);
        }
        let url = parse_download_url(url)?;
        let client = self.inner.client.read().unwrap().clone();
        if let Some(m) = crate::media::resolve(&client, &url).await? {
            return Ok(Some(m));
        }
        let settings = self.settings();
        let tools = &self.inner.tools;
        let program: Option<PathBuf> = if ytdlp::is_known_site(&url) || ytdlp::is_stream_manifest(&url) {
            Some(tools.ensure_ytdlp(&client).await?)
        } else if crate::security::url::looks_like_file_link(url.as_str()).is_none() {
            match tools.ytdlp() {
                Some(path) => {
                    // Only pages and streams are worth asking yt-dlp about.
                    let timeout = Duration::from_secs(settings.read_timeout_secs.min(15));
                    let mime = match tokio::time::timeout(timeout, probe(&client, &url, None)).await {
                        Ok(Ok(info)) => info.mime.unwrap_or_default(),
                        _ => String::new(),
                    };
                    let page = mime.to_ascii_lowercase().starts_with("text/html") || ytdlp::is_stream_mime(&mime);
                    page.then_some(path)
                }
                None => None,
            }
        } else {
            None
        };
        match program {
            Some(path) => ytdlp::resolve(&path, &settings, &url).await,
            None => Ok(None),
        }
    }

    /// The helper program being installed for video support, if any (for "Setting up…" in the UI).
    pub fn tool_setup(&self) -> Option<crate::tools::ToolSetup> {
        self.inner.tools.setup()
    }

    /// Fetch yt-dlp in the background after `delay`, so the first video link doesn't wait for it.
    /// Does nothing when it is already installed or on `PATH`.
    pub fn prepare_media_tools(&self, delay: Duration) {
        let inner = self.inner.clone();
        self.inner.rt.spawn(async move {
            tokio::select! {
                _ = inner.shutdown.cancelled() => return,
                _ = tokio::time::sleep(delay) => {}
            }
            if inner.tools.ytdlp().is_some() {
                return;
            }
            let client = inner.client.read().unwrap().clone();
            if let Err(e) = inner.tools.ensure_ytdlp(&client).await {
                tracing::warn!(error = %e.message, "couldn't prepare yt-dlp in the background");
            }
        });
    }

    /// Check a prospective download for duplicates and name conflicts without adding it.
    pub fn check_add(
        &self,
        url: &str,
        file_name: Option<&str>,
        dest_dir: Option<&str>,
        category: Option<Category>,
    ) -> Result<AddCheck, ErrorInfo> {
        if torrent::is_torrent_link(url) {
            return self.check_add_torrent(url, file_name, dest_dir, category);
        }
        let parsed = parse_download_url(url)?;
        let name = resolve_name(&parsed, file_name);
        let category = category.unwrap_or_else(|| categorize(&name, None));
        let dir = match dest_dir.filter(|d| !d.trim().is_empty()) {
            Some(d) => d.to_string(),
            None => self.inner.dir_for_category(category),
        };
        let dir_path = validate_dest_dir(&dir)?;
        let target = confine(&dir_path, &name)?;
        let records = self.inner.records.lock().unwrap();
        let duplicate_of = records
            .values()
            .find(|r| !r.status.is_finished() && (r.url == parsed.as_str() || r.final_url.as_deref() == Some(parsed.as_str())))
            .cloned();
        let conflicts_with =
            records.values().find(|r| !r.status.is_finished() && same_path(&r.target_path(), &target)).map(|r| r.id.clone());
        let taken = |n: &str| records.values().any(|r| !r.status.is_finished() && same_path(&r.target_path(), &dir_path.join(n)));
        let suggested_name = unique_file_name(&dir_path, &name, &taken);
        Ok(AddCheck {
            duplicate_of,
            file_exists: target.exists(),
            conflicts_with,
            dest_dir: dir,
            file_name: name,
            suggested_name,
            category,
        })
    }

    /// Torrents choose their own file layout when they start, so only duplicates matter here.
    fn check_add_torrent(
        &self,
        url: &str,
        file_name: Option<&str>,
        dest_dir: Option<&str>,
        category: Option<Category>,
    ) -> Result<AddCheck, ErrorInfo> {
        let link = torrent::parse_torrent_link(url)?;
        let name = file_name.map(sanitize_file_name).filter(|n| !n.is_empty()).unwrap_or_else(|| torrent::name_hint(&link));
        let category = category.unwrap_or_else(|| categorize(&name, None));
        let dir = match dest_dir.filter(|d| !d.trim().is_empty()) {
            Some(d) => d.to_string(),
            None => self.inner.dir_for_category(category),
        };
        validate_dest_dir(&dir)?;
        let duplicate_of = self.inner.records.lock().unwrap().values().find(|r| !r.status.is_finished() && r.url == link).cloned();
        Ok(AddCheck {
            duplicate_of,
            file_exists: false,
            conflicts_with: None,
            dest_dir: dir,
            suggested_name: name.clone(),
            file_name: name,
            category,
        })
    }

    pub fn add(&self, req: AddRequest) -> Result<DownloadRecord, ErrorInfo> {
        let is_torrent = torrent::is_torrent_link(&req.url);
        let media_format = req.media_format.as_deref().map(str::trim).filter(|f| !f.is_empty() && f.len() <= 512).map(str::to_string);
        let kind = match (is_torrent, &media_format) {
            (true, _) => DownloadKind::Torrent,
            (false, Some(_)) => DownloadKind::Media,
            (false, None) => DownloadKind::Http,
        };
        let settings = self.settings();
        let given = req.file_name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        let (link, mut name) = if is_torrent {
            // The real name comes from the metadata; Velora picks a free folder when it starts.
            let link = torrent::parse_torrent_link(&req.url)?;
            let name = given.map(sanitize_file_name).unwrap_or_else(|| torrent::name_hint(&link));
            (link, name)
        } else {
            let url = parse_download_url(&req.url)?;
            if let Some(site) = crate::media::unsupported_site(&url) {
                return Err(crate::media::unsupported_error(site));
            }
            let name = resolve_name(&url, given);
            (url.to_string(), name)
        };
        let category = req.category.unwrap_or_else(|| categorize(&name, None));
        let (dest_dir, dest_auto) = match req.dest_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            Some(d) => (d.to_string(), false),
            None => (self.inner.dir_for_category(category), true),
        };
        let dir_path = validate_dest_dir(&dest_dir)?;
        let checksum = match req.checksum.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) if c.len() == 64 && c.chars().all(|ch| ch.is_ascii_hexdigit()) => Some(c.to_ascii_lowercase()),
            Some(_) => return Err(ErrorInfo::new("invalid_checksum", "A SHA-256 checksum is 64 hexadecimal characters.")),
            None => None,
        };
        let id = uuid::Uuid::new_v4().to_string();
        if !req.overwrite && !is_torrent {
            let records = self.inner.records.lock().unwrap();
            let taken = |n: &str| records.values().any(|r| !r.status.is_finished() && same_path(&r.target_path(), &dir_path.join(n)));
            name = unique_file_name(&dir_path, &name, &taken);
        }
        confine(&dir_path, &name)?;

        let status = match req.start {
            StartMode::Now | StartMode::Queue => DownloadStatus::Queued,
            StartMode::Paused => DownloadStatus::Paused,
            StartMode::Schedule(_) => DownloadStatus::Scheduled,
        };
        let queue_pos = self.inner.queue_position_for(req.priority);
        let rec = DownloadRecord {
            id: id.clone(),
            kind,
            url: link,
            media_format,
            info_hash: None,
            final_url: None,
            file_name: name,
            name_auto: given.is_none(),
            dest_dir,
            dest_auto,
            category,
            status,
            priority: req.priority,
            queue_pos,
            total_size: None,
            downloaded: 0,
            resumable: false,
            connections: req.connections.unwrap_or(settings.default_connections).clamp(1, MAX_CONNECTIONS),
            speed_limit: req.speed_limit.filter(|l| *l > 0),
            etag: None,
            last_modified: None,
            mime: None,
            referrer: req.referrer.filter(|r| r.starts_with("http://") || r.starts_with("https://")),
            error: None,
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
            scheduled_at: match req.start {
                StartMode::Schedule(at) => Some(at),
                _ => None,
            },
            checksum,
            checksum_state: ChecksumState::None,
            overwrite: req.overwrite && !is_torrent,
            restart_pending: false,
        };
        self.inner.store.upsert(&rec)?;
        self.inner.records.lock().unwrap().insert(id.clone(), rec.clone());
        self.inner.emit(EngineEvent::Upserted { record: rec.clone() });
        tracing::info!(%id, url = %redact_url(&rec.url), "download added");
        if req.start == StartMode::Now {
            self.inner.start_task(&id, false);
        } else {
            self.inner.wake.notify_one();
        }
        Ok(self.get(&id).unwrap_or(rec))
    }

    // -- Control ---------------------------------------------------------------------------------

    pub fn pause(&self, id: &str) -> Result<(), ErrorInfo> {
        if self.inner.stop_running(id, StopReason::Pause) {
            return Ok(());
        }
        self.inner.update_status(id, |r| match r.status {
            DownloadStatus::Queued | DownloadStatus::Scheduled => {
                r.status = DownloadStatus::Paused;
                true
            }
            _ => false,
        })
    }

    /// Start (or continue) a download now, regardless of the queue limit.
    pub fn resume(&self, id: &str) -> Result<(), ErrorInfo> {
        let status = self.get(id).ok_or_else(not_found)?.status;
        match status {
            s if s.is_running() => Ok(()),
            DownloadStatus::Completed => Err(ErrorInfo::new("invalid_state", "This download has already finished.")),
            _ => {
                self.inner.start_task(id, false);
                Ok(())
            }
        }
    }

    pub fn retry(&self, id: &str) -> Result<(), ErrorInfo> {
        self.resume(id)
    }

    /// Discard partial data and download again from the beginning.
    pub fn restart(&self, id: &str) -> Result<(), ErrorInfo> {
        let rec = self.get(id).ok_or_else(not_found)?;
        if self.inner.stop_running(id, StopReason::Restart) {
            return Ok(());
        }
        // A finished file stays; the new copy gets a free name (HTTP) or is re-checked (torrent).
        if rec.status != DownloadStatus::Completed || rec.kind == DownloadKind::Http {
            delete_partial(&rec);
        }
        self.inner.update(id, &mut |r| {
            r.restart_pending = true;
            r.downloaded = 0;
            r.error = None;
            r.checksum_state = ChecksumState::None;
            r.completed_at = None;
            if r.status == DownloadStatus::Completed {
                r.name_auto = false;
            }
        });
        self.inner.start_task(id, false);
        Ok(())
    }

    /// Stop the download and delete its partial data. The record stays as "cancelled".
    pub fn cancel(&self, id: &str) -> Result<(), ErrorInfo> {
        if self.inner.stop_running(id, StopReason::Cancel) {
            return Ok(());
        }
        let rec = self.get(id).ok_or_else(not_found)?;
        if rec.status == DownloadStatus::Completed {
            return Err(ErrorInfo::new("invalid_state", "This download has already finished."));
        }
        delete_partial(&rec);
        self.inner.update(id, &mut |r| {
            r.status = DownloadStatus::Cancelled;
            r.downloaded = 0;
        });
        Ok(())
    }

    /// Remove records from the list. Partial data is deleted; finished files are never touched.
    pub fn remove(&self, ids: &[String]) -> Result<(), ErrorInfo> {
        let mut handles = HashMap::new();
        {
            let mut running = self.inner.running.lock().unwrap();
            for id in ids {
                if let Some(r) = running.get_mut(id) {
                    r.stop(StopReason::Remove);
                    if let Some(h) = r.handle.take() {
                        handles.insert(id.clone(), h);
                    }
                }
            }
        }
        let mut to_delete = Vec::new();
        {
            let mut records = self.inner.records.lock().unwrap();
            for id in ids {
                if let Some(rec) = records.remove(id) {
                    to_delete.push(rec);
                }
            }
        }
        for rec in &to_delete {
            if rec.status == DownloadStatus::Completed {
                continue;
            }
            match handles.remove(&rec.id) {
                // Delete the partial file once the task has let go of it.
                Some(handle) => {
                    let rec = rec.clone();
                    self.inner.rt.spawn(async move {
                        let _ = handle.await;
                        delete_partial(&rec);
                    });
                }
                None => delete_partial(rec),
            }
        }
        let removed: Vec<String> = to_delete.into_iter().map(|r| r.id).collect();
        self.inner.store.delete(&removed)?;
        self.inner.emit(EngineEvent::Removed { ids: removed });
        Ok(())
    }

    pub fn clear(&self, scope: ClearScope) -> Result<Vec<String>, ErrorInfo> {
        let ids: Vec<String> = self
            .list()
            .into_iter()
            .filter(|r| match scope {
                ClearScope::Completed => r.status == DownloadStatus::Completed,
                ClearScope::Failed => matches!(r.status, DownloadStatus::Failed | DownloadStatus::Cancelled),
                ClearScope::Finished => r.status.is_finished(),
            })
            .map(|r| r.id)
            .collect();
        self.remove(&ids)?;
        Ok(ids)
    }

    /// Pause every running and waiting download and hold the queue.
    pub fn pause_all(&self) {
        self.inner.queue_paused.store(true, Ordering::Release);
        self.inner.persist_queue_state();
        let ids: Vec<String> = self.inner.running.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.inner.stop_running(&id, StopReason::Pause);
        }
        self.inner.emit(EngineEvent::QueueState { paused: true });
    }

    /// Release the queue and move paused downloads back into it.
    pub fn resume_all(&self) {
        self.inner.queue_paused.store(false, Ordering::Release);
        self.inner.persist_queue_state();
        let paused: Vec<String> = self.list().into_iter().filter(|r| r.status == DownloadStatus::Paused).map(|r| r.id).collect();
        for id in paused {
            let _ = self.inner.update_status(&id, |r| {
                r.status = DownloadStatus::Queued;
                true
            });
        }
        self.inner.emit(EngineEvent::QueueState { paused: false });
        self.inner.wake.notify_one();
    }

    pub fn set_priority(&self, id: &str, priority: Priority) -> Result<(), ErrorInfo> {
        let pos = self.inner.queue_position_for(priority);
        self.get(id).ok_or_else(not_found)?;
        self.inner.update(id, &mut |r| {
            r.priority = priority;
            if priority != Priority::Normal {
                r.queue_pos = pos;
            }
        });
        self.inner.wake.notify_one();
        Ok(())
    }

    /// Set the queue order: `ids[0]` runs first.
    pub fn reorder(&self, ids: &[String]) -> Result<(), ErrorInfo> {
        let mut changed = Vec::new();
        {
            let mut records = self.inner.records.lock().unwrap();
            for (i, id) in ids.iter().enumerate() {
                if let Some(r) = records.get_mut(id) {
                    if r.queue_pos != i as i64 {
                        r.queue_pos = i as i64;
                        changed.push(r.clone());
                    }
                }
            }
        }
        self.inner.store.upsert_many(&changed)?;
        for record in changed {
            self.inner.emit(EngineEvent::Upserted { record });
        }
        self.inner.wake.notify_one();
        Ok(())
    }

    pub fn set_speed_limit(&self, id: &str, limit: Option<u64>) -> Result<(), ErrorInfo> {
        let limit = limit.filter(|l| *l > 0);
        self.get(id).ok_or_else(not_found)?;
        if let Some(r) = self.inner.running.lock().unwrap().get(id) {
            r.own_limit.set_rate(limit.unwrap_or(0));
        }
        self.inner.update(id, &mut |r| r.speed_limit = limit);
        Ok(())
    }

    /// Change the connection count; applies the next time the download starts.
    pub fn set_connections(&self, id: &str, connections: u8) -> Result<(), ErrorInfo> {
        self.get(id).ok_or_else(not_found)?;
        self.inner.update(id, &mut |r| r.connections = connections.clamp(1, MAX_CONNECTIONS));
        Ok(())
    }

    pub fn schedule(&self, id: &str, at: Option<i64>) -> Result<(), ErrorInfo> {
        self.inner.stop_running(id, StopReason::Pause);
        self.inner.update_status(id, |r| {
            if r.status == DownloadStatus::Completed {
                return false;
            }
            r.scheduled_at = at;
            r.status = if at.is_some() { DownloadStatus::Scheduled } else { DownloadStatus::Queued };
            true
        })?;
        self.inner.wake.notify_one();
        Ok(())
    }

    /// Rename a download that is not running. Renames the file (or partial files) on disk too.
    pub fn rename(&self, id: &str, new_name: &str) -> Result<DownloadRecord, ErrorInfo> {
        if self.inner.running.lock().unwrap().contains_key(id) {
            return Err(ErrorInfo::new("invalid_state", "Pause the download before renaming it."));
        }
        let rec = self.get(id).ok_or_else(not_found)?;
        let name = sanitize_file_name(new_name);
        if name == rec.file_name {
            return Ok(rec);
        }
        if rec.kind != DownloadKind::Http && rec.status != DownloadStatus::Completed {
            return Err(ErrorInfo::new("invalid_state", "This download can be renamed once it has finished."));
        }
        let dir = validate_dest_dir(&rec.dest_dir)?;
        let new_target = confine(&dir, &name)?;
        if new_target.exists() || self.inner.path_reserved(id, &new_target) {
            return Err(ErrorInfo::new("exists", format!("A file named \"{name}\" already exists in this folder.")));
        }
        let old_target = rec.target_path();
        let io = |e: std::io::Error| ErrorInfo::new("io", format!("Couldn't rename the file: {e}"));
        if rec.status == DownloadStatus::Completed {
            if old_target.exists() {
                std::fs::rename(&old_target, &new_target).map_err(io)?;
            }
        } else {
            for (from, to) in [
                (checkpoint::part_path(&old_target), checkpoint::part_path(&new_target)),
                (checkpoint::checkpoint_path(&old_target), checkpoint::checkpoint_path(&new_target)),
            ] {
                if from.exists() {
                    std::fs::rename(&from, &to).map_err(io)?;
                }
            }
        }
        self.inner.update(id, &mut |r| {
            r.file_name = name.clone();
            r.name_auto = false;
            r.category = categorize(&name, r.mime.as_deref());
        });
        self.get(id).ok_or_else(not_found)
    }

    /// Change the category. Finished files move to the category's folder; downloads that haven't
    /// started follow the category folder; partially downloaded files stay where they are.
    pub fn set_category(&self, id: &str, category: Category) -> Result<DownloadRecord, ErrorInfo> {
        if self.inner.running.lock().unwrap().contains_key(id) {
            return Err(ErrorInfo::new("invalid_state", "Pause the download before moving it."));
        }
        let rec = self.get(id).ok_or_else(not_found)?;
        let new_dir = self.inner.dir_for_category(category);
        let mut dest_dir = rec.dest_dir.clone();
        let mut file_name = rec.file_name.clone();
        if rec.status == DownloadStatus::Completed {
            let dir = validate_dest_dir(&new_dir)?;
            if !same_path(&dir, Path::new(&rec.dest_dir)) {
                std::fs::create_dir_all(&dir).map_err(|e| ErrorInfo::new("io", format!("Couldn't create {}: {e}", dir.display())))?;
                file_name = unique_file_name(&dir, &rec.file_name, &|_| false);
                let target = confine(&dir, &file_name)?;
                move_file(&rec.target_path(), &target).map_err(|e| ErrorInfo::new("io", format!("Couldn't move the file: {e}")))?;
                dest_dir = new_dir;
            }
        } else if rec.downloaded == 0 {
            dest_dir = new_dir;
        }
        self.inner.update(id, &mut |r| {
            r.category = category;
            r.dest_dir = dest_dir.clone();
            r.file_name = file_name.clone();
            r.dest_auto = false;
        });
        self.get(id).ok_or_else(not_found)
    }

    /// Compute the SHA-256 of a finished file. With `expected`, record the comparison result.
    pub async fn verify_checksum(&self, id: &str, expected: Option<String>) -> Result<String, ErrorInfo> {
        let rec = self.get(id).ok_or_else(not_found)?;
        if rec.status != DownloadStatus::Completed {
            return Err(ErrorInfo::new("invalid_state", "Checksums can be computed once the download has finished."));
        }
        let actual = sha256_file(rec.target_path()).await.map_err(|e| ErrorInfo::new("io", format!("Couldn't read the file: {e}")))?;
        let expected = expected.map(|e| e.trim().to_ascii_lowercase()).filter(|e| !e.is_empty()).or(rec.checksum.clone());
        if let Some(exp) = expected {
            let state = if exp == actual { ChecksumState::Verified } else { ChecksumState::Mismatch };
            self.inner.update(id, &mut |r| {
                r.checksum = Some(exp.clone());
                r.checksum_state = state;
            });
        }
        Ok(actual)
    }

    // -- Import / export -------------------------------------------------------------------------

    pub fn export_json(&self) -> String {
        let items: Vec<ExportItem> = self
            .list()
            .into_iter()
            .map(|r| ExportItem {
                kind: r.kind,
                media_format: r.media_format,
                url: r.url,
                file_name: r.file_name,
                dest_dir: r.dest_dir,
                category: r.category,
                status: r.status,
                total_size: r.total_size,
                created_at: r.created_at,
                completed_at: r.completed_at,
                checksum: r.checksum,
            })
            .collect();
        serde_json::to_string_pretty(&ExportFile { format: "velora-downloads".into(), version: 1, items }).unwrap_or_default()
    }

    /// Import a list exported by [`Engine::export_json`]. Unfinished items are added paused; finished
    /// ones are added to history. Returns how many were imported.
    pub fn import_json(&self, json: &str) -> Result<usize, ErrorInfo> {
        let file: ExportFile = serde_json::from_str(json).map_err(|_| ErrorInfo::new("import", "This isn't a Velora export file."))?;
        if file.format != "velora-downloads" {
            return Err(ErrorInfo::new("import", "This isn't a Velora export file."));
        }
        let mut count = 0;
        for item in file.items {
            let finished = item.status == DownloadStatus::Completed;
            if finished {
                let url = if item.kind == DownloadKind::Torrent {
                    torrent::parse_torrent_link(&item.url)
                } else {
                    parse_download_url(&item.url).map(|u| u.to_string())
                };
                let Ok(url) = url else { continue };
                let Ok(dir) = validate_dest_dir(&item.dest_dir) else { continue };
                let name = sanitize_file_name(&item.file_name);
                if confine(&dir, &name).is_err() {
                    continue;
                }
                let rec = DownloadRecord {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: item.kind,
                    url,
                    media_format: item.media_format,
                    info_hash: None,
                    final_url: None,
                    file_name: name,
                    name_auto: false,
                    dest_dir: item.dest_dir,
                    dest_auto: false,
                    category: item.category,
                    status: DownloadStatus::Completed,
                    priority: Priority::Normal,
                    queue_pos: 0,
                    total_size: item.total_size,
                    downloaded: item.total_size.unwrap_or(0),
                    resumable: false,
                    connections: self.settings().default_connections,
                    speed_limit: None,
                    etag: None,
                    last_modified: None,
                    mime: None,
                    referrer: None,
                    error: None,
                    created_at: item.created_at,
                    started_at: None,
                    completed_at: item.completed_at,
                    scheduled_at: None,
                    checksum: item.checksum,
                    checksum_state: ChecksumState::None,
                    overwrite: false,
                    restart_pending: false,
                };
                self.inner.store.upsert(&rec)?;
                self.inner.records.lock().unwrap().insert(rec.id.clone(), rec.clone());
                self.inner.emit(EngineEvent::Upserted { record: rec });
            } else {
                let req = AddRequest {
                    url: item.url,
                    file_name: Some(item.file_name),
                    dest_dir: Some(item.dest_dir),
                    category: Some(item.category),
                    start: StartMode::Paused,
                    checksum: item.checksum,
                    media_format: item.media_format,
                    ..Default::default()
                };
                if self.add(req).is_err() {
                    continue;
                }
            }
            count += 1;
        }
        Ok(count)
    }

    // -- Shutdown --------------------------------------------------------------------------------

    /// Stop all tasks, saving checkpoints. Running downloads resume on next start if the setting
    /// allows it.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        let handles: Vec<JoinHandle<()>> = {
            let mut running = self.inner.running.lock().unwrap();
            running
                .values_mut()
                .filter_map(|r| {
                    r.stop(StopReason::Shutdown);
                    r.handle.take()
                })
                .collect()
        };
        let _ = tokio::time::timeout(Duration::from_secs(8), futures_util::future::join_all(handles)).await;
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    format: String,
    version: u32,
    items: Vec<ExportItem>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportItem {
    #[serde(default)]
    kind: DownloadKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_format: Option<String>,
    url: String,
    file_name: String,
    dest_dir: String,
    category: Category,
    status: DownloadStatus,
    total_size: Option<u64>,
    created_at: i64,
    completed_at: Option<i64>,
    checksum: Option<String>,
}

fn not_found() -> ErrorInfo {
    ErrorInfo::new("not_found", "That download no longer exists.")
}

fn resolve_name(url: &url::Url, given: Option<&str>) -> String {
    match given {
        Some(n) => sanitize_file_name(n),
        None => sanitize_file_name(&name_from_url(url).unwrap_or_else(|| "download".into())),
    }
}

fn delete_partial(rec: &DownloadRecord) {
    match rec.kind {
        DownloadKind::Media => return crate::media::ytdlp::delete_partial(rec),
        DownloadKind::Torrent => return torrent::delete_partial(rec),
        DownloadKind::Http => {}
    }
    let target = rec.target_path();
    for p in [checkpoint::part_path(&target), checkpoint::checkpoint_path(&target)] {
        if let Err(e) = std::fs::remove_file(&p) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %p.display(), error = %e, "couldn't delete partial file");
            }
        }
    }
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Different volume: copy, then remove the original.
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)
        }
    }
}

impl Inner {
    fn emit(&self, ev: EngineEvent) {
        (self.on_event)(ev);
    }

    fn persist_queue_state(&self) {
        let v = if self.queue_paused.load(Ordering::Acquire) { "true" } else { "false" };
        if let Err(e) = self.store.save_value("queue_paused", v) {
            tracing::error!(error = %e.message, "failed to persist queue state");
        }
    }

    fn dir_for_category(&self, category: Category) -> String {
        let s = self.settings.read().unwrap();
        if let Some(dir) = s.category_folders.get(category.key()).filter(|d| !d.trim().is_empty()) {
            return dir.clone();
        }
        if s.auto_categorize {
            Path::new(&s.default_dir).join(category.folder_name()).to_string_lossy().into_owned()
        } else {
            s.default_dir.clone()
        }
    }

    /// High priority goes to the front of the queue, everything else to the back.
    fn queue_position_for(&self, priority: Priority) -> i64 {
        let records = self.records.lock().unwrap();
        let positions = records.values().filter(|r| !r.status.is_finished()).map(|r| r.queue_pos);
        match priority {
            Priority::High => positions.min().map_or(0, |m| m - 1),
            _ => positions.max().map_or(0, |m| m + 1),
        }
    }

    fn update(&self, id: &str, f: &mut dyn FnMut(&mut DownloadRecord)) {
        let rec = {
            let mut records = self.records.lock().unwrap();
            let Some(r) = records.get_mut(id) else { return };
            f(r);
            r.clone()
        };
        if let Err(e) = self.store.upsert(&rec) {
            tracing::error!(error = %e.message, "failed to persist download");
        }
        self.emit(EngineEvent::Upserted { record: rec });
    }

    /// Apply `f` if the record exists; `f` returns whether it changed anything.
    fn update_status(&self, id: &str, f: impl FnOnce(&mut DownloadRecord) -> bool) -> Result<(), ErrorInfo> {
        let rec = {
            let mut records = self.records.lock().unwrap();
            let r = records.get_mut(id).ok_or_else(not_found)?;
            if !f(r) {
                return Ok(());
            }
            r.clone()
        };
        self.store.upsert(&rec)?;
        self.emit(EngineEvent::Upserted { record: rec });
        Ok(())
    }

    /// Signal a running task to stop. Returns `false` if it wasn't running.
    fn stop_running(&self, id: &str, reason: StopReason) -> bool {
        match self.running.lock().unwrap().get(id) {
            Some(r) => {
                r.stop(reason);
                true
            }
            None => false,
        }
    }

    fn apply_speed_limit(&self) {
        let s = self.settings.read().unwrap();
        let now = Local::now();
        let weekday = now.weekday().num_days_from_sunday() as u8;
        let minute = now.hour() * 60 + now.minute();
        let limit = if s.scheduled_limit.window.contains(weekday, minute) { s.scheduled_limit.limit } else { s.global_speed_limit };
        self.global_limit.set_rate(limit);
    }

    fn start_task(self: &Arc<Self>, id: &str, auto: bool) {
        if self.shutdown.is_cancelled() {
            return;
        }
        let mut running = self.running.lock().unwrap();
        if running.contains_key(id) {
            return;
        }
        let rec = {
            let mut records = self.records.lock().unwrap();
            let Some(r) = records.get_mut(id) else { return };
            if r.status == DownloadStatus::Completed && !r.restart_pending {
                return;
            }
            r.status = DownloadStatus::Probing;
            r.error = None;
            r.scheduled_at = None;
            r.clone()
        };
        if let Err(e) = self.store.upsert(&rec) {
            tracing::error!(error = %e.message, "failed to persist download");
        }
        self.emit(EngineEvent::Upserted { record: rec.clone() });

        let live = Arc::new(LiveState::default());
        live.set_total(rec.total_size);
        let own_limit = Arc::new(RateLimiter::new(rec.speed_limit.unwrap_or(0)));
        let cancel = CancellationToken::new();
        let reason = Arc::new(AtomicU8::new(StopReason::None as u8));
        let env = TaskEnv {
            id: id.to_string(),
            client: self.client.read().unwrap().clone(),
            connections: rec.connections,
            settings: self.settings.read().unwrap().clone(),
            global_limit: self.global_limit.clone(),
            own_limit: own_limit.clone(),
            live: live.clone(),
            cancel: cancel.clone(),
            hooks: self.clone(),
            write_fault: self.write_fault.clone(),
        };
        let inner = self.clone();
        let task_id = id.to_string();
        let task_reason = reason.clone();
        let (kind, tools, torrents) = (rec.kind, self.tools.clone(), self.torrents.clone());
        let handle = self.rt.spawn(async move {
            let outcome = match kind {
                DownloadKind::Http => task::run(env).await,
                DownloadKind::Media => crate::media::ytdlp::run(env, tools).await,
                DownloadKind::Torrent => torrent::run(env, torrents).await,
            };
            inner.on_task_finished(&task_id, outcome, StopReason::from_u8(task_reason.load(Ordering::Acquire)));
        });
        tracing::info!(%id, auto, "download started");
        running.insert(id.to_string(), Running { cancel, reason, live, own_limit, auto, handle: Some(handle) });
    }

    fn on_task_finished(&self, id: &str, outcome: TaskOutcome, reason: StopReason) {
        let live = self.running.lock().unwrap().remove(id).map(|r| r.live);
        let exists = self.records.lock().unwrap().contains_key(id);
        if !exists {
            // Removed while running; `remove` deletes the partial file once this task has ended.
            return;
        }
        let rec = self.records.lock().unwrap().get(id).cloned();
        let resumable = rec.as_ref().is_some_and(|r| r.resumable);
        let shutdown_status = if self.settings.read().unwrap().resume_on_startup { DownloadStatus::Queued } else { DownloadStatus::Paused };

        match (outcome, reason) {
            (TaskOutcome::Completed, _) => {
                self.update(id, &mut |r| {
                    r.status = DownloadStatus::Completed;
                    r.completed_at = Some(now_ms());
                    r.error = None;
                });
                if let Some(rec) = self.records.lock().unwrap().get(id).cloned() {
                    tracing::info!(%id, "download completed");
                    self.emit(EngineEvent::Completed { record: rec });
                }
            }
            (TaskOutcome::Failed(info), StopReason::None) => {
                tracing::warn!(%id, code = %info.code, "download failed");
                self.update(id, &mut |r| {
                    r.status = DownloadStatus::Failed;
                    r.error = Some(info.clone());
                    if !r.resumable {
                        r.downloaded = 0;
                    }
                });
                if let Some(rec) = self.records.lock().unwrap().get(id).cloned() {
                    self.emit(EngineEvent::Failed { record: rec });
                }
            }
            (_, reason) => {
                let status = match reason {
                    StopReason::Cancel => DownloadStatus::Cancelled,
                    StopReason::Shutdown => shutdown_status,
                    StopReason::QueueWindow | StopReason::Restart => DownloadStatus::Queued,
                    StopReason::Remove => return,
                    _ => DownloadStatus::Paused,
                };
                if reason == StopReason::Cancel {
                    if let Some(rec) = &rec {
                        delete_partial(rec);
                    }
                }
                let keep_progress = resumable && reason != StopReason::Cancel && reason != StopReason::Restart;
                self.update(id, &mut |r| {
                    r.status = status;
                    if !keep_progress {
                        r.downloaded = 0;
                    }
                    if reason == StopReason::Restart {
                        r.restart_pending = true;
                    }
                });
                if reason == StopReason::Restart {
                    if let Some(rec) = &rec {
                        delete_partial(rec);
                    }
                }
            }
        }
        drop(live);
        self.wake.notify_one();
    }

    /// One scheduler pass: promote scheduled items, apply time windows and limits, fill free slots.
    fn schedule_pass(self: &Arc<Self>) {
        self.apply_speed_limit();
        let now = now_ms();
        let due: Vec<String> = self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.status == DownloadStatus::Scheduled && r.scheduled_at.is_some_and(|t| t <= now))
            .map(|r| r.id.clone())
            .collect();
        for id in due {
            // A scheduled start is an explicit user request: it starts even if the queue is full.
            self.start_task(&id, false);
        }

        let (max_active, window) = {
            let s = self.settings.read().unwrap();
            (s.max_active as usize, s.queue_window.clone())
        };
        let local = Local::now();
        let in_window =
            !window.enabled || window.contains(local.weekday().num_days_from_sunday() as u8, local.hour() * 60 + local.minute());
        if !in_window {
            let auto: Vec<String> = self.running.lock().unwrap().iter().filter(|(_, r)| r.auto).map(|(id, _)| id.clone()).collect();
            for id in auto {
                self.stop_running(&id, StopReason::QueueWindow);
            }
        }
        if self.queue_paused.load(Ordering::Acquire) || !in_window || self.shutdown.is_cancelled() {
            return;
        }

        let active = self.running.lock().unwrap().len();
        if active >= max_active {
            return;
        }
        let mut queued: Vec<(i64, i64, String)> = self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.status == DownloadStatus::Queued)
            .map(|r| (r.queue_pos, r.created_at, r.id.clone()))
            .collect();
        queued.sort();
        for (_, _, id) in queued.into_iter().take(max_active - active) {
            self.start_task(&id, true);
        }
    }

    fn progress_pass(&self, last_was_empty: &mut bool) {
        let now = Instant::now();
        let items: Vec<ProgressUpdate> = {
            let running = self.running.lock().unwrap();
            running.iter().map(|(id, r)| r.live.sample(id, now)).collect()
        };
        let total: u64 = items.iter().map(|p| p.speed).sum();
        if !(items.is_empty() && *last_was_empty) {
            self.emit(EngineEvent::Progress { items: items.clone(), total_speed: total });
        }
        *last_was_empty = items.is_empty();
    }

    fn record_history(&self) {
        let total: u64 = {
            let running = self.running.lock().unwrap();
            running.iter().map(|(id, r)| r.live.sample_peek(id).speed).sum()
        };
        let mut h = self.speed_history.lock().unwrap();
        if h.len() == GLOBAL_HISTORY {
            h.pop_front();
        }
        h.push_back(total);
    }
}

impl TaskHooks for Inner {
    fn record(&self, id: &str) -> Option<DownloadRecord> {
        self.records.lock().unwrap().get(id).cloned()
    }

    fn update(&self, id: &str, f: &mut dyn FnMut(&mut DownloadRecord)) {
        Inner::update(self, id, f);
    }

    fn set_progress(&self, id: &str, downloaded: u64) {
        if let Some(r) = self.records.lock().unwrap().get_mut(id) {
            r.downloaded = downloaded;
        }
    }

    fn path_reserved(&self, id: &str, path: &Path) -> bool {
        self.records.lock().unwrap().values().any(|r| r.id != id && !r.status.is_finished() && same_path(&r.target_path(), path))
    }

    fn dir_for_category(&self, category: Category) -> String {
        Inner::dir_for_category(self, category)
    }
}

async fn scheduler_loop(inner: Arc<Inner>, tick: Duration) {
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_history = Instant::now();
    let mut last_was_empty = true;
    loop {
        let ticked = tokio::select! {
            _ = inner.shutdown.cancelled() => break,
            _ = interval.tick() => true,
            _ = inner.wake.notified() => false,
        };
        if ticked {
            inner.progress_pass(&mut last_was_empty);
            if last_history.elapsed() >= Duration::from_secs(1) {
                last_history = Instant::now();
                inner.record_history();
            }
        }
        inner.schedule_pass();
    }
}
