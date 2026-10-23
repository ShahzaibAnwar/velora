//! X (formerly Twitter) post videos.
//!
//! X plays videos as HLS streams, but for public posts it also publishes plain MP4 files. They are
//! listed by the public endpoint X's own embedded-post widget uses, which needs no account. Private,
//! deleted or age-restricted posts are not available there and are reported as such.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use url::Url;

use super::{MediaVariant, ResolvedMedia};
use crate::model::ErrorInfo;
use crate::security::sanitize_file_name;

pub const SYNDICATION_ENDPOINT: &str = "https://cdn.syndication.twimg.com/tweet-result";
const HOSTS: &[&str] = &["x.com", "www.x.com", "mobile.x.com", "twitter.com", "www.twitter.com", "mobile.twitter.com"];
/// Video files must come from X's media host; anything else in the response is ignored.
const MEDIA_HOST: &str = "video.twimg.com";

/// The post ID in `x.com/<user>/status/<id>`, `x.com/i/status/<id>` or `x.com/i/web/status/<id>`.
pub fn status_id(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if !HOSTS.contains(&host.as_str()) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    let pos = segs.iter().position(|s| *s == "status" || *s == "statuses")?;
    let id = segs.get(pos + 1)?;
    (pos >= 1 && !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.to_string())
}

/// The `token` parameter the embed widget sends: `((id / 1e15) * π).toString(36)` with every `0`
/// and the decimal point removed.
pub fn token(id: &str) -> String {
    let n: f64 = id.parse().unwrap_or(0.0);
    to_radix_string((n / 1e15) * std::f64::consts::PI, 36).chars().filter(|c| *c != '0' && *c != '.').collect()
}

/// Port of V8's `DoubleToRadixCString`, so the result matches JavaScript's `Number#toString(radix)`
/// digit for digit (shortest representation that round-trips).
fn to_radix_string(value: f64, radix: u32) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if !value.is_finite() || value <= 0.0 {
        return "0".into();
    }
    let next_up = |v: f64| f64::from_bits(v.to_bits() + 1);
    let r = radix as f64;
    let mut integer = value.floor();
    let mut fraction = value - integer;
    let mut delta = (0.5 * (next_up(value) - value)).max(next_up(0.0));
    let mut frac: Vec<u32> = Vec::new();
    if fraction >= delta {
        loop {
            fraction *= r;
            delta *= r;
            let digit = fraction as u32;
            frac.push(digit);
            fraction -= digit as f64;
            if (fraction > 0.5 || (fraction == 0.5 && digit & 1 == 1)) && fraction + delta > 1.0 {
                // Round up, propagating the carry through digits already written.
                loop {
                    match frac.pop() {
                        None => {
                            integer += 1.0;
                            break;
                        }
                        Some(d) if d + 1 < radix => {
                            frac.push(d + 1);
                            break;
                        }
                        Some(_) => {}
                    }
                }
                break;
            }
            if fraction < delta {
                break;
            }
        }
    }
    let mut int_digits = Vec::new();
    loop {
        let rem = integer % r;
        int_digits.push(CHARS[rem as usize]);
        integer = (integer - rem) / r;
        if integer <= 0.0 {
            break;
        }
    }
    int_digits.reverse();
    let mut out = String::from_utf8(int_digits).unwrap_or_default();
    if !frac.is_empty() {
        out.push('.');
        out.extend(frac.iter().map(|d| CHARS[*d as usize] as char));
    }
    out
}

#[derive(Deserialize)]
struct Post {
    #[serde(rename = "__typename")]
    typename: Option<String>,
    id_str: Option<String>,
    text: Option<String>,
    user: Option<User>,
    #[serde(rename = "mediaDetails", default)]
    media: Vec<Media>,
}

#[derive(Deserialize)]
struct User {
    screen_name: Option<String>,
}

#[derive(Deserialize)]
struct Media {
    #[serde(rename = "type")]
    kind: String,
    video_info: Option<VideoInfo>,
}

#[derive(Deserialize)]
struct VideoInfo {
    #[serde(default)]
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
struct Variant {
    content_type: String,
    bitrate: Option<u64>,
    url: String,
}

fn unavailable() -> ErrorInfo {
    ErrorInfo::new("not_found", "This X post isn't available to Velora. It may be private, deleted or age-restricted.")
}

/// Resolve an X post to its MP4 files using `endpoint` (overridable for tests).
pub async fn resolve(client: &Client, endpoint: &str, page: &Url) -> Result<ResolvedMedia, ErrorInfo> {
    let id = status_id(page).ok_or_else(unavailable)?;
    let mut api = Url::parse(endpoint).map_err(|_| ErrorInfo::new("internal", "Bad media endpoint."))?;
    api.query_pairs_mut().append_pair("id", &id).append_pair("token", &token(&id)).append_pair("lang", "en");
    let resp = tokio::time::timeout(Duration::from_secs(20), client.get(api).send())
        .await
        .map_err(|_| ErrorInfo::new("timeout", "X took too long to respond."))?
        .map_err(|e| crate::net::retry::from_reqwest(&e).into_info())?;
    match resp.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND => return Err(unavailable()),
        s => return Err(ErrorInfo::new("http", format!("X answered with HTTP {}.", s.as_u16()))),
    }
    let body = resp.bytes().await.map_err(|e| crate::net::retry::from_reqwest(&e).into_info())?;
    if body.len() > 4 * 1024 * 1024 {
        return Err(ErrorInfo::new("http", "X sent an unexpectedly large response."));
    }
    let post: Post = serde_json::from_slice(&body).map_err(|_| unavailable())?;
    if post.typename.as_deref() == Some("TweetTombstone") || post.id_str.is_none() {
        return Err(unavailable());
    }
    let author = post.user.and_then(|u| u.screen_name).filter(|s| !s.is_empty());
    let videos: Vec<Vec<Variant>> = post
        .media
        .into_iter()
        .filter(|m| m.kind == "video" || m.kind == "animated_gif")
        .filter_map(|m| m.video_info.map(|v| v.variants))
        .collect();
    let many = videos.len() > 1;
    let mut variants = Vec::new();
    for (n, list) in videos.into_iter().enumerate() {
        let mut mp4: Vec<Variant> = list
            .into_iter()
            .filter(|v| v.content_type == "video/mp4")
            .filter(|v| Url::parse(&v.url).is_ok_and(|u| u.scheme() == "https" && u.host_str() == Some(MEDIA_HOST)))
            .collect();
        mp4.sort_by_key(|v| std::cmp::Reverse(v.bitrate.unwrap_or(0)));
        for v in mp4 {
            let (width, height) = dimensions(&v.url);
            let size = match (width, height) {
                (Some(w), Some(h)) => format!("{w}×{h}"),
                _ => "MP4".to_string(),
            };
            let label = if many { format!("Video {} · {size}", n + 1) } else { size };
            let stem = match &author {
                Some(a) => format!("{a}_{id}"),
                None => format!("x_{id}"),
            };
            let suffix = if many { format!("_{}", n + 1) } else { String::new() };
            let res = height.map(|h| format!("_{h}p")).unwrap_or_default();
            variants.push(MediaVariant {
                url: v.url,
                label,
                width,
                height,
                bitrate: v.bitrate,
                mime: "video/mp4".into(),
                file_name: sanitize_file_name(&format!("{stem}{suffix}{res}.mp4")),
                format: None,
                size: None,
            });
        }
    }
    if variants.is_empty() {
        return Err(ErrorInfo::new("no_video", "This X post doesn't contain a downloadable video."));
    }
    let title: String = post.text.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect();
    Ok(ResolvedMedia { source: "X".into(), page_url: page.to_string(), title, author, variants })
}

/// `…/vid/avc1/1280x720/abc.mp4` → (1280, 720).
fn dimensions(url: &str) -> (Option<u32>, Option<u32>) {
    let path = url.split('?').next().unwrap_or(url);
    for seg in path.split('/') {
        if let Some((w, h)) = seg.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                return (Some(w), Some(h));
            }
        }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_post_urls() {
        let id = |s: &str| status_id(&Url::parse(s).unwrap());
        assert_eq!(id("https://x.com/SomeUser/status/2112847019228414346").as_deref(), Some("2112847019228414346"));
        assert_eq!(id("https://twitter.com/a/status/20?s=20").as_deref(), Some("20"));
        assert_eq!(id("https://x.com/i/web/status/123/video/1").as_deref(), Some("123"));
        assert_eq!(id("https://mobile.twitter.com/a/status/42/photo/1").as_deref(), Some("42"));
        assert_eq!(id("https://x.com/home"), None);
        assert_eq!(id("https://x.com/a/status/abc"), None);
        assert_eq!(id("https://example.com/a/status/123"), None);
        assert_eq!(id("https://x.com.evil.example/a/status/123"), None);
    }

    #[test]
    fn token_matches_javascript() {
        // Expected values from Node.js: ((Number(id)/1e15)*Math.PI).toString(36).replace(/(0+|\.)/g,'')
        for (id, expected) in [
            ("2112847019228414346", "54dpd99ctj"),
            ("20", "6dq1a2xwd93"),
            ("1460323737035677698", "3jfqq1vhqna"),
            ("1234567890123456789", "2zqic77uqyk"),
            ("1", "bhi2ay3f28n"),
        ] {
            assert_eq!(token(id), expected, "id {id}");
        }
    }

    #[test]
    fn reads_dimensions() {
        assert_eq!(dimensions("https://video.twimg.com/amplify_video/1/vid/avc1/1280x720/a.mp4?tag=14"), (Some(1280), Some(720)));
        assert_eq!(dimensions("https://video.twimg.com/a.mp4"), (None, None));
    }
}
