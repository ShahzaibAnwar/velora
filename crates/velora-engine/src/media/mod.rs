//! Media resolvers: turn a page address into the direct media files it shows.
//!
//! Only sites that publish ordinary, unprotected files for public content are supported. A resolver
//! never logs in, never uses cookies and never touches DRM-protected or private media.

pub mod x;

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

/// Sites whose videos Velora deliberately doesn't download: protected by DRM or obfuscated stream
/// signatures, or only available when signed in. Velora never bypasses either. Only the page hosts
/// are listed; ordinary files linked from these sites are unaffected.
const UNSUPPORTED: &[(&[&str], &str)] = &[
    (&["youtube.com", "youtu.be", "youtube-nocookie.com"], "YouTube"),
    (&["instagram.com"], "Instagram"),
    (&["facebook.com", "fb.watch", "fb.com"], "Facebook"),
    (&["tiktok.com"], "TikTok"),
    (&["netflix.com", "primevideo.com", "disneyplus.com", "hulu.com", "max.com", "hotstar.com"], "this streaming service"),
];

/// The site name if `url` is a page on a site Velora doesn't download videos from.
pub fn unsupported_site(url: &Url) -> Option<&'static str> {
    let host = url.host_str()?.to_ascii_lowercase();
    UNSUPPORTED.iter().find(|(domains, _)| domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))).map(|(_, name)| *name)
}

/// Resolve `url` if a resolver recognises it. `Ok(None)` means "not a supported page"; an error
/// means the page was recognised but its media couldn't be fetched (private, deleted, no video…)
/// or belongs to a site Velora doesn't download from.
pub async fn resolve(client: &Client, url: &Url) -> Result<Option<ResolvedMedia>, ErrorInfo> {
    if let Some(site) = unsupported_site(url) {
        return Err(ErrorInfo::new(
            "unsupported_site",
            format!(
                "Velora doesn't download videos from {site}. They're protected or only available when you're signed in, and Velora never bypasses either."
            ),
        ));
    }
    if x::status_id(url).is_some() {
        return x::resolve(client, x::SYNDICATION_ENDPOINT, url).await.map(Some);
    }
    Ok(None)
}
