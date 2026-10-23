//! Media resolvers: turn a page address into the media it shows.
//!
//! X posts are resolved natively to their MP4 files. Other video sites and HLS/DASH streams go
//! through yt-dlp ([`ytdlp`]). A resolver never logs in, never uses the browser's cookies and never
//! touches DRM-protected media.

pub mod x;
pub mod ytdlp;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::model::ErrorInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaVariant {
    pub url: String,
    /// Human label, e.g. "1280×720".
    pub label: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Bits per second as reported by the site.
    pub bitrate: Option<u64>,
    pub mime: String,
    /// Suggested file name for this variant.
    pub file_name: String,
    /// yt-dlp format selector. When set, `url` is the page and yt-dlp does the download.
    #[serde(default)]
    pub format: Option<String>,
    /// Size in bytes, exact or estimated by the site.
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMedia {
    /// Site name shown to the user, e.g. "X".
    pub source: String,
    pub page_url: String,
    pub title: String,
    pub author: Option<String>,
    /// Best quality first.
    pub variants: Vec<MediaVariant>,
}

/// Streaming services whose catalogue is DRM-protected. Velora never bypasses DRM. Only the page
/// hosts are listed; ordinary files linked from these sites are unaffected.
const UNSUPPORTED: &[(&[&str], &str)] =
    &[(&["netflix.com", "primevideo.com", "disneyplus.com", "hulu.com", "max.com", "hotstar.com"], "this streaming service")];

/// The site name if `url` is a page on a site Velora doesn't download videos from.
pub fn unsupported_site(url: &Url) -> Option<&'static str> {
    let host = url.host_str()?.to_ascii_lowercase();
    UNSUPPORTED.iter().find(|(domains, _)| domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))).map(|(_, name)| *name)
}

pub fn unsupported_error(site: &str) -> ErrorInfo {
    ErrorInfo::new(
        "unsupported_site",
        format!("Velora doesn't download videos from {site}. Its videos are DRM-protected, and Velora never bypasses DRM."),
    )
}

/// Resolve `url` with the built-in resolvers. `Ok(None)` means none of them recognises the page;
/// an error means the page was recognised but its media couldn't be fetched (private, deleted, no
/// video…) or belongs to a site Velora doesn't download from.
pub async fn resolve(client: &Client, url: &Url) -> Result<Option<ResolvedMedia>, ErrorInfo> {
    if let Some(site) = unsupported_site(url) {
        return Err(unsupported_error(site));
    }
    if x::status_id(url).is_some() {
        return x::resolve(client, x::SYNDICATION_ENDPOINT, url).await.map(Some);
    }
    Ok(None)
}
