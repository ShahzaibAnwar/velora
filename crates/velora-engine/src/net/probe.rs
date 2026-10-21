//! Discover what a URL points to before downloading: size, range support, validators and file name.

use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_RANGE, CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE, REFERER,
};
use reqwest::{Client, Response, StatusCode};
use url::Url;

use super::content_disposition;
use super::range::{parse_content_range, parse_unsatisfied_total};
use super::retry::{from_reqwest, from_status, Failure};
use crate::category::{categorize, extension_for_mime};
use crate::model::ProbeInfo;
use crate::security::filename::{sanitize_file_name, split_ext};

/// Probe `url` with a one-byte range request.
///
/// Range support is only reported when the server answers `206` with a `Content-Range` that starts
/// at 0 and states the complete length. A `200` means the server ignores ranges.
pub async fn probe(client: &Client, url: &Url, referrer: Option<&str>) -> Result<ProbeInfo, Failure> {
    let resp = send(client, url, referrer, true).await?;
    let resp = match resp.status() {
        StatusCode::PARTIAL_CONTENT | StatusCode::OK => resp,
        StatusCode::RANGE_NOT_SATISFIABLE => {
            // An empty file answers `bytes=0-0` with 416 and `bytes */0`.
            let total = header_str(&resp, CONTENT_RANGE).and_then(|v| parse_unsatisfied_total(&v));
            if total == Some(0) {
                return Ok(build_info(url, &resp, Some(0), false));
            }
            send(client, url, referrer, false).await?
        }
        StatusCode::BAD_REQUEST => {
            // A few servers reject any Range header; try once without it.
            send(client, url, referrer, false).await?
        }
        _ => resp,
    };

    let status = resp.status();
    if !status.is_success() {
        return Err(from_status(status, resp.headers()));
    }

    let encoded = header_str(&resp, CONTENT_ENCODING).filter(|v| !v.eq_ignore_ascii_case("identity"));
    let (size, resumable) = if status == StatusCode::PARTIAL_CONTENT {
        match header_str(&resp, CONTENT_RANGE).and_then(|v| parse_content_range(&v)) {
            Some(cr) if cr.start == 0 && cr.total.is_some() => (cr.total, encoded.is_none()),
            _ => (None, false),
        }
    } else if encoded.is_none() {
        (resp.content_length(), false)
    } else {
        (None, false)
    };

    Ok(build_info(url, &resp, size, resumable))
}

async fn send(client: &Client, url: &Url, referrer: Option<&str>, ranged: bool) -> Result<Response, Failure> {
    let mut req = client.get(url.clone()).header(ACCEPT_ENCODING, "identity");
    if ranged {
        req = req.header(RANGE, "bytes=0-0");
    }
    if let Some(r) = referrer.filter(|r| r.starts_with("http://") || r.starts_with("https://")) {
        req = req.header(REFERER, r);
    }
    req.send().await.map_err(|e| from_reqwest(&e))
}

fn header_str(resp: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    resp.headers().get(name).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string())
}

fn build_info(original: &Url, resp: &Response, size: Option<u64>, resumable: bool) -> ProbeInfo {
    let final_url = resp.url().clone();
    let mime = header_str(resp, CONTENT_TYPE);
    let file_name = derive_file_name(resp, &final_url, mime.as_deref());
    ProbeInfo {
        url: original.to_string(),
        final_url: final_url.to_string(),
        host: final_url.host_str().unwrap_or_default().to_string(),
        category: categorize(&file_name, mime.as_deref()),
        file_name,
        size,
        mime,
        resumable,
        etag: header_str(resp, ETAG),
        last_modified: header_str(resp, LAST_MODIFIED),
        content_encoding: header_str(resp, CONTENT_ENCODING).filter(|v| !v.eq_ignore_ascii_case("identity")),
    }
}

fn derive_file_name(resp: &Response, final_url: &Url, mime: Option<&str>) -> String {
    let from_header = resp.headers().get(CONTENT_DISPOSITION).and_then(|v| content_disposition::file_name(v.as_bytes()));
    let raw = from_header.or_else(|| name_from_url(final_url)).unwrap_or_else(|| "download".to_string());
    let mut name = sanitize_file_name(&raw);
    if split_ext(&name).1.is_none() {
        if let Some(ext) = mime.and_then(extension_for_mime) {
            name = format!("{name}.{ext}");
        }
    }
    name
}

/// The last non-empty path segment of a URL, percent-decoded.
pub fn name_from_url(url: &Url) -> Option<String> {
    let segment = url.path_segments()?.rfind(|s| !s.is_empty())?;
    let decoded = percent_encoding::percent_decode_str(segment).decode_utf8_lossy().into_owned();
    (!decoded.trim().is_empty()).then_some(decoded)
}
