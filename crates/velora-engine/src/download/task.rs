//! One run of one download: probe → (segmented | single stream) → verify → finalise.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, ETAG, IF_RANGE, RANGE, REFERER};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::checkpoint::{self, Checkpoint};
use super::live::LiveState;
use super::partfile::{available_space, PartFile, WriteFault};
use super::segment::{initial_segments, Segment};
use crate::model::{Category, ChecksumState, DownloadRecord, DownloadStatus, ErrorInfo, ProbeInfo};
use crate::net::range::parse_content_range;
use crate::net::retry::{backoff_delay, from_io, from_reqwest, from_status, Failure};
use crate::net::{probe, RateLimiter};
use crate::security::{confine, parse_download_url, unique_file_name, validate_dest_dir};
use crate::settings::Settings;
use crate::util::now_ms;

/// Bytes buffered per connection before a positional write.
const WRITE_BUFFER: usize = 512 * 1024;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);

/// The engine-side services a task needs. Implemented by the engine; mocked in tests if needed.
pub trait TaskHooks: Send + Sync {
    fn record(&self, id: &str) -> Option<DownloadRecord>;
    /// Mutate, persist and broadcast a record.
    fn update(&self, id: &str, f: &mut dyn FnMut(&mut DownloadRecord));
    /// Update the in-memory downloaded counter without persisting (the checkpoint is authoritative).
    fn set_progress(&self, id: &str, downloaded: u64);
    /// Another unfinished download already intends to write to `path`.
    fn path_reserved(&self, id: &str, path: &Path) -> bool;
    fn dir_for_category(&self, category: Category) -> String;
}

pub struct TaskEnv {
    pub id: String,
    pub client: Client,
    pub connections: u8,
    pub settings: Settings,
    pub global_limit: Arc<RateLimiter>,
    pub own_limit: Arc<RateLimiter>,
    pub live: Arc<LiveState>,
    pub cancel: CancellationToken,
    pub hooks: Arc<dyn TaskHooks>,
    pub write_fault: Option<WriteFault>,
}

#[derive(Debug)]
pub enum TaskOutcome {
    Completed,
    Cancelled,
    Failed(ErrorInfo),
}

#[derive(Debug)]
enum TaskError {
    Cancelled,
    Failed(ErrorInfo),
}

impl From<Failure> for TaskError {
    fn from(f: Failure) -> Self {
        TaskError::Failed(f.into_info())
    }
}

impl From<ErrorInfo> for TaskError {
    fn from(e: ErrorInfo) -> Self {
        TaskError::Failed(e)
    }
}

fn internal(message: &str) -> TaskError {
    TaskError::Failed(ErrorInfo::new("internal", message))
}

pub async fn run(env: TaskEnv) -> TaskOutcome {
    match run_inner(&env).await {
        Ok(()) => TaskOutcome::Completed,
        Err(TaskError::Cancelled) => TaskOutcome::Cancelled,
        Err(TaskError::Failed(info)) => TaskOutcome::Failed(info),
    }
}

async fn run_inner(env: &TaskEnv) -> Result<(), TaskError> {
    let rec = env.hooks.record(&env.id).ok_or_else(|| internal("The download record is missing."))?;
    let url = parse_download_url(&rec.url)?;
    let info = probe_with_retries(env, &url, rec.referrer.as_deref()).await?;
    let final_url = Url::parse(&info.final_url).unwrap_or_else(|_| url.clone());

    // Settle the file name and folder. A server-provided name is only adopted before any data exists.
    let restart = rec.restart_pending;
    let adopt_name = rec.name_auto && (rec.downloaded == 0 || restart);
    let mut file_name = rec.file_name.clone();
    let mut dest_dir = rec.dest_dir.clone();
    let mut category = rec.category;
    if adopt_name {
        file_name = info.file_name.clone();
        category = info.category;
        if rec.dest_auto {
            dest_dir = env.hooks.dir_for_category(category);
        }
    }
    let dest = validate_dest_dir(&dest_dir)?;
    tokio::fs::create_dir_all(&dest).await.map_err(|e| from_io(&e, "creating the download folder"))?;
    if adopt_name && !rec.overwrite {
        let id = env.id.clone();
        file_name = unique_file_name(&dest, &file_name, &|n| env.hooks.path_reserved(&id, &dest.join(n)));
    }
    let target = confine(&dest, &file_name)?;
    let part_path = checkpoint::part_path(&target);
    let ck_path = checkpoint::checkpoint_path(&target);

    env.hooks.update(&env.id, &mut |r| {
        r.file_name = file_name.clone();
        r.dest_dir = dest_dir.clone();
        r.category = category;
        r.name_auto = false;
        r.final_url = Some(info.final_url.clone());
        r.total_size = info.size;
        r.mime = info.mime.clone();
        r.resumable = info.resumable;
        r.etag = info.etag.clone();
        r.last_modified = info.last_modified.clone();
        r.status = DownloadStatus::Downloading;
        r.restart_pending = false;
        r.error = None;
        r.started_at.get_or_insert_with(now_ms);
    });

    if restart {
        let _ = std::fs::remove_file(&part_path);
        let _ = std::fs::remove_file(&ck_path);
    }
    env.live.set_total(info.size);

    match info.size {
        Some(size) if info.resumable && size > 0 => {
            let resume = match checkpoint::load(&ck_path) {
                Some(ck) if ck.id == env.id && ck.is_consistent() && ck.written() > 0 => {
                    if !ck.matches_remote(&info) {
                        return Err(TaskError::Failed(ErrorInfo::new(
                            "remote_changed",
                            "The file on the server changed since this download started. Restart it to download the new version.",
                        )));
                    }
                    let part_ok = std::fs::metadata(&part_path).map(|m| m.len() == size).unwrap_or(false);
                    if !part_ok {
                        tracing::warn!(id = %env.id, "partial data missing or wrong size; starting over");
                    }
                    part_ok.then_some(ck)
                }
                _ => None,
            };
            let remaining = size - resume.as_ref().map_or(0, Checkpoint::written);
            check_space(&dest, remaining)?;
            run_segmented(env, &info, &final_url, &rec, &part_path, &ck_path, resume).await?;
        }
        _ => {
            if let Some(size) = info.size {
                check_space(&dest, size)?;
            }
            let _ = std::fs::remove_file(&ck_path);
            run_single(env, &info, &final_url, &rec, &part_path).await?;
        }
    }

    finalize(env, &rec, &info, &dest, &file_name, &part_path, &ck_path).await
}

fn check_space(dir: &Path, needed: u64) -> Result<(), TaskError> {
    match available_space(dir) {
        Some(free) if free < needed => Err(TaskError::Failed(ErrorInfo::new(
            "disk_full",
            format!("Not enough free space: {} needed, {} available.", crate::util::format_bytes(needed), crate::util::format_bytes(free)),
        ))),
        _ => Ok(()),
    }
}

async fn sleep_or_cancel(cancel: &CancellationToken, d: Duration) -> Result<(), TaskError> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TaskError::Cancelled),
        _ = tokio::time::sleep(d) => Ok(()),
    }
}

async fn probe_with_retries(env: &TaskEnv, url: &Url, referrer: Option<&str>) -> Result<ProbeInfo, TaskError> {
    let timeout = Duration::from_secs(env.settings.read_timeout_secs);
    let cap = Duration::from_secs(env.settings.retry_max_delay_secs);
    let mut attempt = 0;
    loop {
        let res = tokio::select! {
            _ = env.cancel.cancelled() => return Err(TaskError::Cancelled),
            r = tokio::time::timeout(timeout, probe(&env.client, url, referrer)) => {
                r.unwrap_or_else(|_| Err(Failure::transient("timeout", "The server took too long to respond.")))
            }
        };
        match res {
            Ok(info) => return Ok(info),
            Err(Failure::Permanent(info)) => return Err(TaskError::Failed(info)),
            Err(Failure::Transient { info, retry_after }) => {
                attempt += 1;
                if attempt > env.settings.max_retries {
                    return Err(TaskError::Failed(info));
                }
                tracing::debug!(id = %env.id, attempt, code = %info.code, "probe failed, retrying");
                sleep_or_cancel(&env.cancel, retry_after.unwrap_or_else(|| backoff_delay(attempt, cap))).await?;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Segmented (range) mode
// ---------------------------------------------------------------------------------------------------

struct RangeCtx {
    id: String,
    client: Client,
    url: Url,
    referrer: Option<String>,
    /// Value for `If-Range`: a strong ETag, else Last-Modified.
    validator: Option<String>,
    etag: Option<String>,
    size: u64,
    part: PartFile,
    live: Arc<LiveState>,
    global_limit: Arc<RateLimiter>,
    own_limit: Arc<RateLimiter>,
    cancel: CancellationToken,
    max_retries: u32,
    retry_cap: Duration,
    read_timeout: Duration,
}

async fn run_segmented(
    env: &TaskEnv,
    info: &ProbeInfo,
    url: &Url,
    rec: &DownloadRecord,
    part_path: &Path,
    ck_path: &Path,
    resume: Option<Checkpoint>,
) -> Result<(), TaskError> {
    let size = info.size.unwrap_or(0);
    let part = PartFile::open(part_path, Some(size), resume.is_none(), env.write_fault.clone())
        .map_err(|e| from_io(&e, "creating the partial file"))?;
    let segments: Vec<Arc<Segment>> = match &resume {
        Some(ck) => ck.segments.iter().map(|s| Arc::new(Segment::new(s.start, s.end, s.written))).collect(),
        None => initial_segments(size, env.connections),
    };
    env.live.set_single(false);
    env.live.table.set(segments);

    let base = Checkpoint::new(&env.id, info, Vec::new());
    write_checkpoint(&part, ck_path, &base, &env.live).await?;

    let strong_etag = info.etag.clone().filter(|e| !e.starts_with("W/"));
    let child = env.cancel.child_token();
    let ctx = Arc::new(RangeCtx {
        id: env.id.clone(),
        client: env.client.clone(),
        url: url.clone(),
        referrer: rec.referrer.clone(),
        validator: strong_etag.or_else(|| info.last_modified.clone()),
        etag: info.etag.clone(),
        size,
        part: part.clone(),
        live: env.live.clone(),
        global_limit: env.global_limit.clone(),
        own_limit: env.own_limit.clone(),
        cancel: child.clone(),
        max_retries: env.settings.max_retries,
        retry_cap: Duration::from_secs(env.settings.retry_max_delay_secs),
        read_timeout: Duration::from_secs(env.settings.read_timeout_secs),
    });

    let mut workers = JoinSet::new();
    for _ in 0..env.connections.max(1) {
        workers.spawn(range_worker(ctx.clone()));
    }

    // Periodic checkpoints while the workers run.
    let stop = CancellationToken::new();
    let ticker = {
        let (stop, part, base, live, hooks, id) =
            (stop.clone(), part.clone(), base.clone(), env.live.clone(), env.hooks.clone(), env.id.clone());
        let ck_path = ck_path.to_path_buf();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = tokio::time::sleep(CHECKPOINT_INTERVAL) => {
                        match write_checkpoint(&part, &ck_path, &base, &live).await {
                            Ok(written) => hooks.set_progress(&id, written),
                            Err(e) => tracing::warn!(%id, error = ?e, "checkpoint failed"),
                        }
                    }
                }
            }
        })
    };

    let mut first_err = None;
    while let Some(joined) = workers.join_next().await {
        let res = joined
            .unwrap_or_else(|e| Err(TaskError::Failed(ErrorInfo::new("internal", format!("A download worker stopped unexpectedly: {e}")))));
        if let Err(e) = res {
            if first_err.is_none() {
                child.cancel();
                first_err = Some(e);
            }
        }
    }
    stop.cancel();
    let _ = ticker.await;
    let written = write_checkpoint(&part, ck_path, &base, &env.live).await?;
    env.hooks.set_progress(&env.id, written);

    if env.cancel.is_cancelled() {
        return Err(TaskError::Cancelled);
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    if !env.live.table.all_done() {
        return Err(internal("Some parts of the file are still missing."));
    }
    Ok(())
}

/// Flush the part file, then atomically record the segment state that was durable *before* the flush.
/// Returns the number of bytes recorded.
async fn write_checkpoint(part: &PartFile, path: &Path, base: &Checkpoint, live: &LiveState) -> Result<u64, TaskError> {
    let states = live.table.states();
    part.sync().await.map_err(|e| from_io(&e, "saving the partial file"))?;
    let mut ck = base.clone();
    ck.segments = states;
    let written = ck.written();
    checkpoint::save_async(path.to_path_buf(), ck).await.map_err(|e| from_io(&e, "saving download progress"))?;
    Ok(written)
}

async fn range_worker(ctx: Arc<RangeCtx>) -> Result<(), TaskError> {
    while let Some(seg) = ctx.live.table.next_work() {
        ctx.live.conn_opened();
        let res = fetch_segment(&ctx, &seg).await;
        ctx.live.conn_closed();
        seg.set_active(false);
        res?;
        if ctx.cancel.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
    }
    Ok(())
}

async fn fetch_segment(ctx: &RangeCtx, seg: &Segment) -> Result<(), TaskError> {
    let mut attempt = 0u32;
    loop {
        if ctx.cancel.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
        let before = seg.claimed();
        match fetch_range(ctx, seg).await {
            Ok(()) => return Ok(()),
            Err(TaskErrorOrRetry::Task(e)) => return Err(e),
            Err(TaskErrorOrRetry::Retry { info, retry_after }) => {
                let progressed = seg.claimed() > before;
                attempt = if progressed { 0 } else { attempt + 1 };
                if attempt > ctx.max_retries {
                    return Err(TaskError::Failed(info));
                }
                let delay =
                    retry_after.unwrap_or_else(
                        || {
                            if progressed {
                                Duration::from_millis(250)
                            } else {
                                backoff_delay(attempt, ctx.retry_cap)
                            }
                        },
                    );
                tracing::debug!(id = %ctx.id, attempt, code = %info.code, ?delay, "segment retry");
                sleep_or_cancel(&ctx.cancel, delay).await?;
            }
        }
    }
}

/// Result of one HTTP request attempt: fatal/cancelled, or worth retrying after a delay.
enum TaskErrorOrRetry {
    Task(TaskError),
    Retry { info: ErrorInfo, retry_after: Option<Duration> },
}

impl From<TaskError> for TaskErrorOrRetry {
    fn from(e: TaskError) -> Self {
        TaskErrorOrRetry::Task(e)
    }
}

fn retry(f: Failure) -> TaskErrorOrRetry {
    match f {
        Failure::Transient { info, retry_after } => TaskErrorOrRetry::Retry { info, retry_after },
        Failure::Permanent(info) => TaskErrorOrRetry::Task(TaskError::Failed(info)),
    }
}

async fn fetch_range(ctx: &RangeCtx, seg: &Segment) -> Result<(), TaskErrorOrRetry> {
    let Some((pos, end)) = seg.pending() else {
        return Ok(());
    };
    let mut req = ctx.client.get(ctx.url.clone()).header(RANGE, format!("bytes={pos}-{end}")).header(ACCEPT_ENCODING, "identity");
    if let Some(v) = &ctx.validator {
        req = req.header(IF_RANGE, v.as_str());
    }
    if let Some(r) = &ctx.referrer {
        req = req.header(REFERER, r.as_str());
    }

    let resp = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(TaskError::Cancelled.into()),
        r = tokio::time::timeout(ctx.read_timeout, req.send()) => match r {
            Err(_) => return Err(retry(Failure::transient("timeout", "The server took too long to respond."))),
            Ok(Err(e)) => return Err(retry(from_reqwest(&e))),
            Ok(Ok(resp)) => resp,
        },
    };

    match resp.status() {
        StatusCode::PARTIAL_CONTENT => {
            let cr = resp.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range);
            let valid = cr.is_some_and(|cr| cr.start == pos && cr.end <= end && cr.total == Some(ctx.size));
            if !valid {
                // Never stitch bytes from a range we did not ask for.
                return Err(retry(Failure::transient("bad_range", "The server sent the wrong part of the file.")));
            }
            let resp_etag = resp.headers().get(ETAG).and_then(|v| v.to_str().ok());
            if let (Some(ours), Some(theirs)) = (ctx.etag.as_deref(), resp_etag) {
                if ours != theirs {
                    return Err(TaskError::Failed(remote_changed()).into());
                }
            }
        }
        StatusCode::OK => {
            // A full response to a ranged request: the resource changed (If-Range failed) or the server
            // stopped honouring ranges. Either way these bytes don't belong at `pos`.
            return Err(TaskError::Failed(if ctx.validator.is_some() {
                remote_changed()
            } else {
                ErrorInfo::new("no_ranges", "The server stopped accepting partial requests. Restart the download.")
            })
            .into());
        }
        StatusCode::RANGE_NOT_SATISFIABLE => return Err(TaskError::Failed(remote_changed()).into()),
        status => {
            return Err(match from_status(status, resp.headers()) {
                Failure::Permanent(info) => TaskError::Failed(info).into(),
                transient => retry(transient),
            })
        }
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(WRITE_BUFFER);
    let mut buf_offset = pos;
    let outcome: Result<(), TaskErrorOrRetry> = loop {
        let next = tokio::select! {
            _ = ctx.cancel.cancelled() => break Err(TaskError::Cancelled.into()),
            n = tokio::time::timeout(ctx.read_timeout, stream.next()) => n,
        };
        let chunk = match next {
            Err(_) => break Err(retry(Failure::transient("timeout", "The server stopped sending data."))),
            Ok(None) => break Ok(()),
            Ok(Some(Err(e))) => break Err(retry(from_reqwest(&e))),
            Ok(Some(Ok(chunk))) => chunk,
        };
        tokio::select! {
            _ = ctx.cancel.cancelled() => break Err(TaskError::Cancelled.into()),
            _ = async { ctx.global_limit.acquire(chunk.len()).await; ctx.own_limit.acquire(chunk.len()).await } => {}
        }
        let (offset, take) = seg.claim(chunk.len());
        if take == 0 {
            break Ok(());
        }
        debug_assert_eq!(offset, buf_offset + buf.len() as u64);
        buf.extend_from_slice(&chunk[..take]);
        ctx.live.add_received(take as u64);
        if buf.len() >= WRITE_BUFFER {
            if let Err(e) = flush(ctx, seg, &mut buf, &mut buf_offset).await {
                break Err(e);
            }
        }
        if take < chunk.len() {
            break Ok(());
        }
    };
    // Whatever happened, persist the bytes we already hold.
    let flushed = flush(ctx, seg, &mut buf, &mut buf_offset).await;
    outcome?;
    flushed?;
    if seg.is_done() {
        Ok(())
    } else {
        Err(retry(Failure::transient("network", "The connection closed before the part was complete.")))
    }
}

async fn flush(ctx: &RangeCtx, seg: &Segment, buf: &mut Vec<u8>, buf_offset: &mut u64) -> Result<(), TaskErrorOrRetry> {
    if buf.is_empty() {
        return Ok(());
    }
    let len = buf.len() as u64;
    let data = std::mem::take(buf);
    let (mut data, res) = ctx.part.write_at(*buf_offset, data).await;
    res.map_err(|e| TaskError::from(from_io(&e, "writing to disk")))?;
    *buf_offset += len;
    seg.mark_written(*buf_offset);
    data.clear();
    *buf = data;
    Ok(())
}

fn remote_changed() -> ErrorInfo {
    ErrorInfo::new("remote_changed", "The file on the server changed since this download started. Restart it to download the new version.")
}

// ---------------------------------------------------------------------------------------------------
// Single-stream mode (no range support, unknown size, or encoded content)
// ---------------------------------------------------------------------------------------------------

async fn run_single(env: &TaskEnv, info: &ProbeInfo, url: &Url, rec: &DownloadRecord, part_path: &Path) -> Result<(), TaskError> {
    env.live.set_single(true);
    env.live.set_total(info.size);
    let part = PartFile::open(part_path, None, true, env.write_fault.clone()).map_err(|e| from_io(&e, "creating the partial file"))?;
    let cap = Duration::from_secs(env.settings.retry_max_delay_secs);
    let mut attempt = 0;
    loop {
        if env.cancel.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
        part.truncate().await.map_err(|e| from_io(&e, "preparing the partial file"))?;
        env.live.set_single_bytes(0);
        match single_attempt(env, &part, url, rec).await {
            Ok(written) => {
                if let Some(size) = info.size {
                    if written != size {
                        return Err(TaskError::Failed(ErrorInfo::new(
                            "incomplete",
                            format!("The server sent {written} bytes but announced {size}."),
                        )));
                    }
                }
                part.sync().await.map_err(|e| from_io(&e, "saving the file"))?;
                return Ok(());
            }
            Err(TaskErrorOrRetry::Task(e)) => return Err(e),
            Err(TaskErrorOrRetry::Retry { info, retry_after }) => {
                attempt += 1;
                if attempt > env.settings.max_retries {
                    return Err(TaskError::Failed(info));
                }
                sleep_or_cancel(&env.cancel, retry_after.unwrap_or_else(|| backoff_delay(attempt, cap))).await?;
            }
        }
    }
}

async fn single_attempt(env: &TaskEnv, part: &PartFile, url: &Url, rec: &DownloadRecord) -> Result<u64, TaskErrorOrRetry> {
    let read_timeout = Duration::from_secs(env.settings.read_timeout_secs);
    let mut req = env.client.get(url.clone()).header(ACCEPT_ENCODING, "identity");
    if let Some(r) = &rec.referrer {
        req = req.header(REFERER, r.as_str());
    }
    env.live.conn_opened();
    let _guard = ConnGuard(&env.live);
    let resp = tokio::select! {
        _ = env.cancel.cancelled() => return Err(TaskError::Cancelled.into()),
        r = tokio::time::timeout(read_timeout, req.send()) => match r {
            Err(_) => return Err(retry(Failure::transient("timeout", "The server took too long to respond."))),
            Ok(Err(e)) => return Err(retry(from_reqwest(&e))),
            Ok(Ok(resp)) => resp,
        },
    };
    if resp.status() != StatusCode::OK {
        return Err(match from_status(resp.status(), resp.headers()) {
            Failure::Permanent(info) => TaskError::Failed(info).into(),
            transient => retry(transient),
        });
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(WRITE_BUFFER);
    let mut written = 0u64;
    loop {
        let next = tokio::select! {
            _ = env.cancel.cancelled() => return Err(TaskError::Cancelled.into()),
            n = tokio::time::timeout(read_timeout, stream.next()) => n,
        };
        match next {
            Err(_) => return Err(retry(Failure::transient("timeout", "The server stopped sending data."))),
            Ok(None) => break,
            Ok(Some(Err(e))) => return Err(retry(from_reqwest(&e))),
            Ok(Some(Ok(chunk))) => {
                tokio::select! {
                    _ = env.cancel.cancelled() => return Err(TaskError::Cancelled.into()),
                    _ = async { env.global_limit.acquire(chunk.len()).await; env.own_limit.acquire(chunk.len()).await } => {}
                }
                buf.extend_from_slice(&chunk);
                env.live.add_received(chunk.len() as u64);
                env.live.set_single_bytes(written + buf.len() as u64);
                if buf.len() >= WRITE_BUFFER {
                    written = single_flush(part, &mut buf, written).await?;
                }
            }
        }
    }
    single_flush(part, &mut buf, written).await
}

async fn single_flush(part: &PartFile, buf: &mut Vec<u8>, offset: u64) -> Result<u64, TaskErrorOrRetry> {
    if buf.is_empty() {
        return Ok(offset);
    }
    let len = buf.len() as u64;
    let (mut data, res) = part.write_at(offset, std::mem::take(buf)).await;
    res.map_err(|e| TaskError::from(from_io(&e, "writing to disk")))?;
    data.clear();
    *buf = data;
    Ok(offset + len)
}

struct ConnGuard<'a>(&'a LiveState);
impl Drop for ConnGuard<'_> {
    fn drop(&mut self) {
        self.0.conn_closed();
    }
}

// ---------------------------------------------------------------------------------------------------
// Finalisation
// ---------------------------------------------------------------------------------------------------

async fn finalize(
    env: &TaskEnv,
    rec: &DownloadRecord,
    info: &ProbeInfo,
    dest: &Path,
    file_name: &str,
    part_path: &Path,
    ck_path: &Path,
) -> Result<(), TaskError> {
    let len = std::fs::metadata(part_path).map_err(|e| from_io(&e, "checking the finished file"))?.len();
    if let Some(size) = info.size {
        if len != size {
            return Err(TaskError::Failed(ErrorInfo::new("incomplete", "The downloaded file is not the expected size.")));
        }
    }

    if let Some(expected) = rec.checksum.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        env.hooks.update(&env.id, &mut |r| {
            r.status = DownloadStatus::Verifying;
            r.checksum_state = ChecksumState::Pending;
        });
        let actual = sha256_file(part_path.to_path_buf()).await.map_err(|e| from_io(&e, "verifying the checksum"))?;
        let state = if actual.eq_ignore_ascii_case(expected) { ChecksumState::Verified } else { ChecksumState::Mismatch };
        env.hooks.update(&env.id, &mut |r| r.checksum_state = state);
    }

    let mut name = file_name.to_string();
    let mut target = confine(dest, &name)?;
    if target.exists() {
        if rec.overwrite {
            tokio::fs::remove_file(&target).await.map_err(|e| from_io(&e, "replacing the existing file"))?;
        } else {
            let id = env.id.clone();
            name = unique_file_name(dest, &name, &|n| env.hooks.path_reserved(&id, &dest.join(n)));
            target = confine(dest, &name)?;
        }
    }
    tokio::fs::rename(part_path, &target).await.map_err(|e| from_io(&e, "moving the finished file into place"))?;
    let _ = tokio::fs::remove_file(ck_path).await;

    env.hooks.update(&env.id, &mut |r| {
        r.file_name = name.clone();
        r.downloaded = len;
        r.total_size = Some(len);
    });
    Ok(())
}

pub async fn sha256_file(path: PathBuf) -> std::io::Result<String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut f = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    })
    .await
    .map_err(std::io::Error::other)?
}
