//! Video pages and streaming manifests (HLS `.m3u8`, DASH `.mpd`) through yt-dlp.
//!
//! Resolving runs `yt-dlp -J` and turns the format list into a few quality choices. Downloading
//! runs yt-dlp with the chosen format selector, reads a machine-readable progress line it prints
//! for us, and merges separate video/audio streams with ffmpeg. yt-dlp keeps `.part` files, so a
//! paused or interrupted download continues where it stopped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use url::Url;

use super::{MediaVariant, ResolvedMedia};
use crate::category::categorize;
use crate::download::task::{TaskEnv, TaskOutcome};
use crate::model::{DownloadRecord, DownloadStatus, ErrorInfo};
use crate::process::ToolProcess;
use crate::security::{confine, sanitize_file_name, unique_file_name, validate_dest_dir};
use crate::settings::{ProxyMode, Settings};
use crate::tools::Tools;
use crate::util::now_ms;

const RESOLVE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_JSON: usize = 64 * 1024 * 1024;
/// Marks our progress lines among yt-dlp's other output.
const PROGRESS_TAG: &str = "[velora-progress]";
const FILE_TAG: &str = "[velora-file]";
/// Resolutions offered as separate choices, highest first.
const MAX_CHOICES: usize = 6;

/// Sites whose pages are worth handing to yt-dlp straight away. Other HTML pages are tried only
/// when yt-dlp is already installed.
const KNOWN_SITES: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "youtube-nocookie.com",
    "vimeo.com",
    "dailymotion.com",
    "dai.ly",
    "twitch.tv",
    "soundcloud.com",
    "instagram.com",
    "facebook.com",
    "fb.watch",
    "tiktok.com",
    "reddit.com",
    "v.redd.it",
    "bilibili.com",
    "b23.tv",
    "streamable.com",
    "bandcamp.com",
    "mixcloud.com",
    "rumble.com",
    "odysee.com",
    "ted.com",
    "vk.com",
    "ok.ru",
    "nicovideo.jp",
    "kick.com",
    "threads.net",
    "pinterest.com",
    "bsky.app",
];

pub fn is_known_site(url: &Url) -> bool {
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else { return false };
    KNOWN_SITES.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// An HLS or DASH manifest, recognised by its extension.
pub fn is_stream_manifest(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    path.ends_with(".m3u8") || path.ends_with(".mpd")
}

pub fn is_stream_mime(mime: &str) -> bool {
    let m = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    matches!(m.as_str(), "application/vnd.apple.mpegurl" | "application/x-mpegurl" | "audio/mpegurl" | "application/dash+xml")
}

fn network_args(settings: &Settings) -> Vec<String> {
    let mut args = vec!["--socket-timeout".to_string(), settings.read_timeout_secs.to_string()];
    match settings.proxy.mode {
        ProxyMode::Manual if !settings.proxy.url.trim().is_empty() => args.extend(["--proxy".into(), settings.proxy.url.trim().into()]),
        ProxyMode::None => args.extend(["--proxy".into(), String::new()]),
        _ => {}
    }
    args
}

/// Ask yt-dlp what `url` offers. `Ok(None)` means yt-dlp doesn't recognise the page.
pub async fn resolve(ytdlp: &Path, settings: &Settings, url: &Url) -> Result<Option<ResolvedMedia>, ErrorInfo> {
    let mut args = vec!["--ignore-config".to_string(), "-J".into(), "--no-playlist".into(), "--no-warnings".into(), "--no-color".into()];
    args.extend(network_args(settings));
    args.extend(["--".into(), url.to_string()]);
    let mut proc = ToolProcess::spawn(ytdlp, &args).map_err(|e| ErrorInfo::new("tool_run", format!("Couldn't start yt-dlp: {e}")))?;
    let mut stdout = proc.child.stdout.take().expect("piped").take(MAX_JSON as u64);
    let mut stderr = proc.child.stderr.take().expect("piped");
    let collect = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (a, b) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        a.and(b)?;
        let status = proc.child.wait().await?;
        Ok::<_, std::io::Error>((status, out, err))
    };
    let (status, out, err) = match tokio::time::timeout(RESOLVE_TIMEOUT, collect).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(ErrorInfo::new("tool_run", format!("yt-dlp failed: {e}"))),
        Err(_) => {
            proc.kill().await;
            return Err(ErrorInfo::new("timeout", "The site took too long to describe this video."));
        }
    };
    let stderr = String::from_utf8_lossy(&err);
    if !status.success() {
        if stderr.contains("Unsupported URL") {
            return Ok(None);
        }
        return Err(readable_error(&stderr));
    }
    let json: Value =
        serde_json::from_slice(&out).map_err(|_| ErrorInfo::new("tool_run", "yt-dlp returned something Velora couldn't read."))?;
    if json.get("_type").and_then(Value::as_str) == Some("playlist") {
        return Err(ErrorInfo::new("playlist", "This link is a playlist or channel. Open a single video and copy its link instead."));
    }
    Ok(Some(media_from_info(&json, url)))
}

/// Turn yt-dlp's info JSON into a short list of choices, best first.
pub(crate) fn media_from_info(info: &Value, page: &Url) -> ResolvedMedia {
    let s = |k: &str| info.get(k).and_then(Value::as_str).map(str::to_string);
    let title = s("title").unwrap_or_else(|| "video".into());
    let source = s("extractor_key").map(|k| pretty_source(&k)).unwrap_or_else(|| page.host_str().unwrap_or("web").to_string());
    let author = s("uploader").or_else(|| s("channel")).or_else(|| s("uploader_id"));
    let stem = sanitize_file_name(&title);
    let formats: Vec<&Value> = info.get("formats").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();

    fn text<'a>(f: &'a Value, k: &str) -> &'a str {
        f.get(k).and_then(Value::as_str).unwrap_or("")
    }
    let num = |f: &Value, k: &str| f.get(k).and_then(Value::as_f64);
    let has_video = |f: &Value| !matches!(text(f, "vcodec"), "none");
    let has_audio = |f: &Value| !matches!(text(f, "acodec"), "none");
    let size_of = |f: &Value| num(f, "filesize").or_else(|| num(f, "filesize_approx")).map(|v| v as u64);

    let best_audio = formats
        .iter()
        .filter(|f| has_audio(f) && !has_video(f))
        .max_by(|a, b| num(a, "abr").unwrap_or(0.0).total_cmp(&num(b, "abr").unwrap_or(0.0)));
    let separate_streams = best_audio.is_some() && formats.iter().any(|f| has_video(f) && !has_audio(f));

    let mut variants = Vec::new();
    let heights: BTreeSet<u64> = formats.iter().filter(|f| has_video(f)).filter_map(|f| num(f, "height").map(|h| h as u64)).collect();
    for h in heights.iter().rev().take(MAX_CHOICES) {
        let best = formats
            .iter()
            .filter(|f| has_video(f) && num(f, "height").map(|v| v as u64) == Some(*h))
            .max_by(|a, b| num(a, "tbr").unwrap_or(0.0).total_cmp(&num(b, "tbr").unwrap_or(0.0)));
        let Some(best) = best else { continue };
        let width = num(best, "width").map(|w| w as u32);
        let (format, ext, size) = if separate_streams {
            let audio_size = best_audio.and_then(|a| size_of(a));
            (
                format!("bv*[height={h}][ext=mp4]+ba[ext=m4a]/bv*[height={h}]+ba/b[height={h}]"),
                "mp4".to_string(),
                size_of(best).map(|v| v + audio_size.unwrap_or(0)),
            )
        } else {
            let ext = text(best, "ext");
            (format!("b[height={h}]/bv*[height={h}]+ba"), if ext.is_empty() { "mp4".into() } else { ext.to_string() }, size_of(best))
        };
        variants.push(MediaVariant {
            url: page.to_string(),
            label: format!("{h}p"),
            width,
            height: Some(*h as u32),
            bitrate: num(best, "tbr").map(|k| (k * 1000.0) as u64),
            mime: format!("video/{ext}"),
            file_name: sanitize_file_name(&format!("{stem} ({h}p).{ext}")),
            format: Some(format),
            size,
        });
    }
    if variants.is_empty() && formats.iter().any(|f| has_video(f)) {
        // Streams without a known resolution (common for HLS): offer "best".
        variants.push(MediaVariant {
            url: page.to_string(),
            label: "Best available".into(),
            width: None,
            height: None,
            bitrate: None,
            mime: "video/mp4".into(),
            file_name: sanitize_file_name(&format!("{stem}.mp4")),
            format: Some("bv*+ba/b".into()),
            size: None,
        });
    }
    if let Some(audio) = best_audio {
        let ext = match text(audio, "ext") {
            "" => "m4a",
            e => e,
        };
        variants.push(MediaVariant {
            url: page.to_string(),
            label: format!("Audio only · {ext}"),
            width: None,
            height: None,
            bitrate: num(audio, "abr").map(|k| (k * 1000.0) as u64),
            mime: format!("audio/{ext}"),
            file_name: sanitize_file_name(&format!("{stem}.{ext}")),
            format: Some("ba[ext=m4a]/ba".into()),
            size: size_of(audio),
        });
    }
    if variants.is_empty() {
        // Audio-only sites and single-file pages without a format list.
        let ext = s("ext").unwrap_or_else(|| "mp4".into());
        variants.push(MediaVariant {
            url: page.to_string(),
            label: "Best available".into(),
            width: None,
            height: None,
            bitrate: None,
            mime: format!("{}/{ext}", if formats.iter().any(|f| has_video(f)) || formats.is_empty() { "video" } else { "audio" }),
            file_name: sanitize_file_name(&format!("{stem}.{ext}")),
            format: Some("b/bv*+ba".into()),
            size: num(info, "filesize").or_else(|| num(info, "filesize_approx")).map(|v| v as u64),
        });
    }
    ResolvedMedia { source, page_url: page.to_string(), title, author, variants }
}

fn pretty_source(key: &str) -> String {
    match key {
        "Youtube" | "YoutubeTab" => "YouTube".into(),
        "Generic" => "web page".into(),
        "TikTok" => "TikTok".into(),
        "SoundcloudSet" | "Soundcloud" => "SoundCloud".into(),
        k => k.to_string(),
    }
}

/// The last `ERROR:` line yt-dlp printed, phrased for people.
fn readable_error(stderr: &str) -> ErrorInfo {
    let line = stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("ERROR:"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("");
    let msg = line.trim_start_matches("ERROR:").trim();
    // yt-dlp prefixes `[extractor] id: `; drop it.
    let msg = match msg.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((_, rest)) => rest.split_once(": ").map_or(rest, |(_, m)| m),
        None => msg,
    };
    let lower = msg.to_ascii_lowercase();
    let (code, text) = if lower.contains("drm") {
        ("drm", "This video is protected by DRM and can't be downloaded.".to_string())
    } else if ["sign in", "login", "log in", "logged-in", "logged in", "cookies", "credentials", "private"]
        .iter()
        .any(|k| lower.contains(k))
    {
        ("sign_in", "This video is private or needs you to be signed in, so it can't be downloaded.".to_string())
    } else if lower.contains("not available") || lower.contains("removed") || lower.contains("404") {
        ("unavailable", "This video isn't available any more.".to_string())
    } else if msg.is_empty() {
        ("tool_run", "yt-dlp couldn't read this page.".to_string())
    } else {
        ("tool_run", msg.chars().take(300).collect())
    };
    ErrorInfo::new(code, text)
}

// ---------------------------------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------------------------------

pub async fn run(env: TaskEnv, tools: std::sync::Arc<Tools>) -> TaskOutcome {
    match run_inner(&env, &tools).await {
        Ok(()) => TaskOutcome::Completed,
        Err(None) => TaskOutcome::Cancelled,
        Err(Some(info)) => TaskOutcome::Failed(info),
    }
}

/// `Err(None)` is a cancellation.
async fn run_inner(env: &TaskEnv, tools: &Tools) -> Result<(), Option<ErrorInfo>> {
    let rec = env.hooks.record(&env.id).ok_or_else(|| Some(ErrorInfo::new("internal", "The download record is missing.")))?;
    let format = rec.media_format.clone().unwrap_or_else(|| "bv*+ba/b".into());
    let page = Url::parse(&rec.url).map_err(|_| Some(ErrorInfo::new("invalid_url", "This doesn't look like a valid web address.")))?;

    let ytdlp = cancellable(env, tools.ensure_ytdlp(&env.client)).await?;
    let needs_ffmpeg = format.contains('+') || is_stream_manifest(&page);
    let ffmpeg = if needs_ffmpeg { Some(cancellable(env, tools.ensure_ffmpeg(&env.client)).await?) } else { tools.ffmpeg_dir() };

    let dest = validate_dest_dir(&rec.dest_dir).map_err(Some)?;
    tokio::fs::create_dir_all(&dest).await.map_err(|e| Some(ErrorInfo::new("io", format!("Couldn't create the download folder: {e}"))))?;
    let mut file_name = rec.file_name.clone();
    if rec.downloaded == 0 && !rec.overwrite {
        let id = env.id.clone();
        file_name = unique_file_name(&dest, &file_name, &|n| env.hooks.path_reserved(&id, &dest.join(n)));
    }
    confine(&dest, &file_name).map_err(Some)?;
    let stem = stem_of(&file_name);
    env.hooks.update(&env.id, &mut |r| {
        r.file_name = file_name.clone();
        r.status = DownloadStatus::Downloading;
        r.resumable = true;
        r.restart_pending = false;
        r.error = None;
        r.started_at.get_or_insert_with(now_ms);
    });

    let mut args: Vec<String> = vec![
        // Must come first. yt-dlp otherwise reads `yt-dlp.conf` from the `-P` folder, i.e. the
        // download folder, where a downloaded file could plant options such as `--exec`.
        "--ignore-config".into(),
        "--newline".into(),
        "--no-color".into(),
        "--no-playlist".into(),
        "--no-mtime".into(),
        "--quiet".into(),
        "--progress".into(),
        "--progress-template".into(),
        format!(
            "download:{PROGRESS_TAG} %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.status)s"
        ),
        "--print".into(),
        format!("after_move:{FILE_TAG} %(filepath)s"),
        "-f".into(),
        format,
        "--merge-output-format".into(),
        "mp4".into(),
        "-N".into(),
        env.connections.clamp(1, 16).to_string(),
        "-P".into(),
        dest.to_string_lossy().into_owned(),
        "-o".into(),
        format!("{}.%(ext)s", stem.replace('%', "%%")),
    ];
    if rec.restart_pending || rec.overwrite {
        args.push("--force-overwrites".into());
    }
    if let Some(dir) = &ffmpeg {
        args.extend(["--ffmpeg-location".into(), dir.to_string_lossy().into_owned()]);
    }
    let limit = [rec.speed_limit.unwrap_or(0), env.global_limit.rate()].into_iter().filter(|l| *l > 0).min();
    if let Some(l) = limit {
        args.extend(["--limit-rate".into(), l.to_string()]);
    }
    if let Some(r) = &rec.referrer {
        args.extend(["--referer".into(), r.clone()]);
    }
    args.extend(network_args(&env.settings));
    args.extend(["--".into(), page.to_string()]);

    env.live.set_single(true);
    env.live.set_total(rec.total_size);
    let mut proc =
        ToolProcess::spawn(&ytdlp, &args).map_err(|e| Some(ErrorInfo::new("tool_run", format!("Couldn't start yt-dlp: {e}"))))?;
    env.live.conn_opened();
    let _conn = ConnGuard(env);

    let stdout = proc.child.stdout.take().expect("piped");
    let mut stderr = proc.child.stderr.take().expect("piped");
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = (&mut stderr).take(1024 * 1024).read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut tracker = StreamProgress::default();
    let mut final_path: Option<PathBuf> = None;
    let read = async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(rest) = line.strip_prefix(FILE_TAG) {
                final_path = Some(PathBuf::from(rest.trim()));
            } else if let Some(rest) = line.strip_prefix(PROGRESS_TAG) {
                if let Some((done, total)) = tracker.update(rest) {
                    let before = env.live.downloaded();
                    env.live.set_single_bytes(done);
                    env.live.add_received(done.saturating_sub(before));
                    env.hooks.set_progress(&env.id, done);
                    if total.is_some() {
                        env.live.set_total(total);
                    }
                }
            }
        }
    };
    tokio::select! {
        _ = env.cancel.cancelled() => {
            proc.kill().await;
            err_task.abort();
            return Err(None);
        }
        _ = read => {}
    }
    let status = proc.child.wait().await.map_err(|e| Some(ErrorInfo::new("tool_run", format!("yt-dlp failed: {e}"))))?;
    let stderr = err_task.await.unwrap_or_default();
    if env.cancel.is_cancelled() {
        return Err(None);
    }
    if !status.success() {
        return Err(Some(readable_error(&stderr)));
    }

    let path = final_path
        .filter(|p| p.is_file())
        .or_else(|| find_output(&dest, &stem))
        .ok_or_else(|| Some(ErrorInfo::new("incomplete", "yt-dlp finished but the saved file couldn't be found.")))?;
    let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(file_name);
    env.hooks.update(&env.id, &mut |r| {
        r.file_name = name.clone();
        r.category = categorize(&name, None);
        r.downloaded = len;
        r.total_size = Some(len);
    });
    Ok(())
}

async fn cancellable<T>(env: &TaskEnv, fut: impl std::future::Future<Output = Result<T, ErrorInfo>>) -> Result<T, Option<ErrorInfo>> {
    tokio::select! {
        _ = env.cancel.cancelled() => Err(None),
        r = fut => r.map_err(Some),
    }
}

struct ConnGuard<'a>(&'a TaskEnv);
impl Drop for ConnGuard<'_> {
    fn drop(&mut self) {
        self.0.live.conn_closed();
    }
}

/// yt-dlp reports each stream (video, then audio) from zero. Add finished streams up so the bar only
/// moves forward.
#[derive(Default)]
struct StreamProgress {
    base: u64,
    last_done: u64,
    last_total: u64,
}

impl StreamProgress {
    /// Parse `<downloaded> <total> <estimate> <status>`; returns overall `(done, total)`.
    fn update(&mut self, rest: &str) -> Option<(u64, Option<u64>)> {
        let mut it = rest.split_whitespace();
        let num = |s: Option<&str>| s.and_then(|v| v.parse::<f64>().ok()).map(|v| v as u64);
        let done = num(it.next())?;
        let total = num(it.next());
        let estimate = num(it.next());
        let total = total.or(estimate);
        if done + 1024 * 1024 < self.last_done {
            // A new stream started.
            self.base += self.last_total.max(self.last_done);
        }
        self.last_done = done;
        self.last_total = total.unwrap_or(done);
        Some((self.base + done, total.map(|t| self.base + t)))
    }
}

fn stem_of(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 5 => stem.to_string(),
        _ => file_name.to_string(),
    }
}

fn find_output(dir: &Path, stem: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        p.is_file() && name.strip_prefix(stem).is_some_and(|r| r.starts_with('.') && !is_temp_name(r))
    })
}

/// yt-dlp's leftovers for `<stem>`: `.part`, `.ytdl`, fragment files and per-format streams such as
/// `<stem>.f137.mp4` that exist only until the merge.
fn is_temp_name(rest: &str) -> bool {
    let lower = rest.to_ascii_lowercase();
    lower.ends_with(".part")
        || lower.ends_with(".ytdl")
        || lower.contains(".part-frag")
        || lower.contains(".temp.")
        || lower.strip_prefix(".f").is_some_and(|r| {
            r.split('.').next().is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
                && r.contains('.')
        })
}

/// Delete yt-dlp's partial files for a media download that was cancelled or removed.
pub fn delete_partial(rec: &DownloadRecord) {
    let stem = stem_of(&rec.file_name);
    let Ok(entries) = std::fs::read_dir(&rec.dest_dir) else { return };
    for p in entries.flatten().map(|e| e.path()) {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if p.is_file() && name.strip_prefix(&stem).is_some_and(|r| r.starts_with('.') && is_temp_name(r)) {
            if let Err(e) = std::fs::remove_file(&p) {
                tracing::warn!(path = %p.display(), error = %e, "couldn't delete partial media file");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_accumulates_streams() {
        let mut p = StreamProgress::default();
        assert_eq!(p.update("1000 10000000 NA downloading"), Some((1000, Some(10_000_000))));
        assert_eq!(p.update("10000000 10000000 NA finished"), Some((10_000_000, Some(10_000_000))));
        // Audio stream starts from zero.
        assert_eq!(p.update("500 2000000 NA downloading"), Some((10_000_500, Some(12_000_000))));
        assert_eq!(p.update("NA NA NA downloading"), None);
        assert_eq!(p.update("2048.0 NA 4096.0 downloading"), Some((10_002_048, Some(10_004_096))));
    }

    #[test]
    fn temp_names() {
        assert!(is_temp_name(".mp4.part"));
        assert!(is_temp_name(".f137.mp4"));
        assert!(is_temp_name(".f137.mp4.part"));
        assert!(is_temp_name(".mp4.ytdl"));
        assert!(is_temp_name(".temp.mp4"));
        assert!(!is_temp_name(".mp4"));
        assert!(!is_temp_name(".flac"));
        assert_eq!(stem_of("Clip (1080p).mp4"), "Clip (1080p)");
    }

    #[test]
    fn variants_from_split_streams() {
        let info = serde_json::json!({
            "title": "My Clip", "extractor_key": "Youtube", "uploader": "someone",
            "formats": [
                {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "abr": 128.0, "filesize": 1000},
                {"format_id": "137", "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 1080, "width": 1920, "tbr": 4000.0, "filesize": 9000},
                {"format_id": "22", "ext": "mp4", "vcodec": "avc1", "acodec": "mp4a", "height": 720, "width": 1280, "tbr": 2000.0}
            ]
        });
        let page = Url::parse("https://www.youtube.com/watch?v=x").unwrap();
        let m = media_from_info(&info, &page);
        assert_eq!(m.source, "YouTube");
        assert_eq!(m.variants.len(), 3);
        assert_eq!(m.variants[0].height, Some(1080));
        assert_eq!(m.variants[0].size, Some(10_000));
        assert_eq!(m.variants[0].file_name, "My Clip (1080p).mp4");
        assert!(m.variants[0].format.as_deref().unwrap().contains("+ba"));
        assert!(m.variants[2].label.starts_with("Audio only"));
        assert!(m.variants.iter().all(|v| v.url == page.as_str()));
    }

    #[test]
    fn errors_are_readable() {
        let e = readable_error("WARNING: x\nERROR: [youtube] abc: Private video. Sign in if you've been granted access\n");
        assert_eq!(e.code, "sign_in");
        assert_eq!(readable_error("ERROR: [generic] Unable to download: HTTP Error 404").code, "unavailable");
        assert_eq!(readable_error("ERROR: [vimeo] 1: The web client only works when logged-in. Use --cookies").code, "sign_in");
    }
}
