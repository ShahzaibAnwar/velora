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
use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use reqwest::Client;
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

pub struct Tools {
    /// `<app data>/tools`. `None` disables installing (tests); helpers on `PATH` are still used.
    dir: Option<PathBuf>,
    ytdlp_lock: Mutex<()>,
    ffmpeg_lock: Mutex<()>,
    refresh_started: AtomicBool,
}

fn tool_error(what: &str, detail: impl std::fmt::Display) -> ErrorInfo {
    ErrorInfo::new("tool_install", format!("Couldn't set up {what}: {detail}"))
}

impl Tools {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, ytdlp_lock: Mutex::new(()), ffmpeg_lock: Mutex::new(()), refresh_started: AtomicBool::new(false) }
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
        install_ytdlp(client, &target).await?;
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
        install_ffmpeg(client, &dir).await?;
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
            match install_ytdlp(&client, &path).await {
                Ok(()) => tracing::info!("yt-dlp updated"),
                Err(e) => tracing::warn!(error = %e.message, "yt-dlp update failed"),
            }
        });
    }
}

async fn install_ytdlp(client: &Client, target: &Path) -> Result<(), ErrorInfo> {
    let dir = target.parent().ok_or_else(|| tool_error("yt-dlp", "invalid folder"))?;
    std::fs::create_dir_all(dir).map_err(|e| tool_error("yt-dlp", e))?;
    let tmp = dir.join(format!("yt-dlp{EXE}.download"));
    let expected = published_hash(client, &format!("{YTDLP_BASE}/SHA2-256SUMS"), YTDLP_ASSET).await.map_err(|e| tool_error("yt-dlp", e))?;
    let actual =
        fetch_to_file(client, &format!("{YTDLP_BASE}/{YTDLP_ASSET}"), &tmp, MAX_YTDLP_BYTES).await.map_err(|e| tool_error("yt-dlp", e))?;
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

async fn install_ffmpeg(client: &Client, dir: &Path) -> Result<(), ErrorInfo> {
    let parent = dir.parent().ok_or_else(|| tool_error("ffmpeg", "invalid folder"))?;
    std::fs::create_dir_all(parent).map_err(|e| tool_error("ffmpeg", e))?;
    let zip_path = parent.join("ffmpeg.zip.download");
    let expected =
        published_hash(client, &format!("{FFMPEG_BASE}/checksums.sha256"), FFMPEG_ASSET).await.map_err(|e| tool_error("ffmpeg", e))?;
    let actual = fetch_to_file(client, &format!("{FFMPEG_BASE}/{FFMPEG_ASSET}"), &zip_path, MAX_FFMPEG_BYTES)
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
        let Some(file_name) = entry.name().rsplit('/').next().map(str::to_ascii_lowercase) else { continue };
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
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("the checksum list returned HTTP {}", resp.status().as_u16()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    parse_sums(&text, asset).ok_or_else(|| format!("{asset} isn't listed in the published checksums"))
}

pub(crate) fn parse_sums(text: &str, asset: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

/// Stream `url` into `path`, returning the SHA-256 of what was written.
async fn fetch_to_file(client: &Client, url: &str, path: &Path, max: u64) -> Result<String, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("the download returned HTTP {}", resp.status().as_u16()));
    }
    if resp.content_length().is_some_and(|n| n > max) {
        return Err("the file is unexpectedly large".into());
    }
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        total += chunk.len() as u64;
        if total > max {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err("the file is unexpectedly large".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| e.to_string())?;
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
}
