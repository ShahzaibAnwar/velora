//! Error classification and retry timing.

use std::error::Error as _;
use std::io;
use std::time::{Duration, SystemTime};

use rand::Rng;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;

use crate::model::ErrorInfo;

const MAX_RETRY_AFTER: Duration = Duration::from_secs(300);
const BASE_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub enum Failure {
    /// Worth retrying after a delay.
    Transient { info: ErrorInfo, retry_after: Option<Duration> },
    /// Retrying will not help; fail the download.
    Permanent(ErrorInfo),
}

impl Failure {
    pub fn info(&self) -> &ErrorInfo {
        match self {
            Failure::Transient { info, .. } | Failure::Permanent(info) => info,
        }
    }

    pub fn into_info(self) -> ErrorInfo {
        match self {
            Failure::Transient { info, .. } | Failure::Permanent(info) => info,
        }
    }

    pub fn transient(code: &str, message: impl Into<String>) -> Self {
        Failure::Transient { info: ErrorInfo::new(code, message), retry_after: None }
    }

    pub fn permanent(code: &str, message: impl Into<String>) -> Self {
        Failure::Permanent(ErrorInfo::new(code, message))
    }
}

pub fn from_reqwest(err: &reqwest::Error) -> Failure {
    if is_tls_error(err) {
        return Failure::permanent(
            "tls",
            "The secure connection couldn't be verified (certificate problem). Velora never skips certificate checks.",
        );
    }
    if err.is_redirect() {
        return Failure::permanent("too_many_redirects", "The server redirected too many times.");
    }
    if err.is_builder() {
        return Failure::permanent("invalid_url", "This link can't be requested.");
    }
    if err.is_timeout() {
        return Failure::transient("timeout", "The server took too long to respond.");
    }
    if err.is_connect() {
        let host = err.url().and_then(|u| u.host_str()).unwrap_or("the server");
        return Failure::transient("network", format!("Couldn't connect to {host}. Check your internet connection."));
    }
    Failure::transient("network", "The connection was interrupted.")
}

fn is_tls_error(err: &reqwest::Error) -> bool {
    let mut source = err.source();
    while let Some(s) = source {
        let text = s.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("handshake") || text.contains("sec_e_") || text.contains("cert_e_") {
            return true;
        }
        source = s.source();
    }
    false
}

pub fn from_status(status: StatusCode, headers: &HeaderMap) -> Failure {
    let code = status.as_u16();
    let retry_after = parse_retry_after(headers);
    let transient = |code: &str, message: String| Failure::Transient { info: ErrorInfo::new(code, message), retry_after };
    match code {
        408 => transient("timeout", "The server timed out the request (HTTP 408).".into()),
        425 | 429 => transient("rate_limited", format!("The server is limiting requests (HTTP {code}). Velora will wait and retry.")),
        500 | 502 | 503 | 504 => transient("server", format!("The server had a temporary problem (HTTP {code}).")),
        401 | 407 => Failure::permanent(
            "unauthorized",
            format!("The server requires a login (HTTP {code}). Download this file from your browser instead."),
        ),
        403 => {
            Failure::permanent("forbidden", "The server refused access (HTTP 403). The link may have expired or need a browser session.")
        }
        404 => Failure::permanent("not_found", "The file was not found on the server (HTTP 404)."),
        410 => Failure::permanent("gone", "The file is no longer available (HTTP 410)."),
        451 => Failure::permanent("forbidden", "The file is unavailable for legal reasons (HTTP 451)."),
        416 => Failure::permanent("bad_range", "The server rejected the requested byte range (HTTP 416)."),
        c if (500..600).contains(&c) => transient("server", format!("The server returned an error (HTTP {c}).")),
        c => Failure::permanent("http", format!("The server answered with HTTP {c}.")),
    }
}

pub fn from_io(err: &io::Error, context: &str) -> Failure {
    // ERROR_DISK_FULL (112) and ERROR_HANDLE_DISK_FULL (39) on Windows, ENOSPC (28) elsewhere.
    let disk_full = matches!(err.raw_os_error(), Some(112) | Some(39)) || (cfg!(unix) && err.raw_os_error() == Some(28));
    if disk_full || err.kind() == io::ErrorKind::StorageFull {
        return Failure::permanent("disk_full", "There isn't enough free space on the disk.");
    }
    match err.kind() {
        io::ErrorKind::PermissionDenied => {
            Failure::permanent("permission", format!("Windows denied access while {context}. Choose a folder you can write to."))
        }
        io::ErrorKind::NotFound => Failure::permanent("path", format!("A folder or file disappeared while {context}.")),
        _ => Failure::permanent("io", format!("A disk error occurred while {context}: {err}")),
    }
}

/// `Retry-After` as either delta-seconds or an HTTP date, capped at five minutes.
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let delay = if let Ok(secs) = value.parse::<u64>() {
        Duration::from_secs(secs)
    } else {
        let when = httpdate::parse_http_date(value).ok()?;
        when.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO)
    };
    Some(delay.min(MAX_RETRY_AFTER))
}

/// Exponential backoff with full jitter: a random delay in `[base/2, min(cap, base·2^attempt)]`.
pub fn backoff_delay(attempt: u32, cap: Duration) -> Duration {
    let exp = BASE_DELAY.saturating_mul(1u32 << attempt.min(16));
    let upper = exp.min(cap).max(BASE_DELAY);
    let lower = BASE_DELAY / 2;
    let millis = rand::thread_rng().gen_range(lower.as_millis() as u64..=upper.as_millis() as u64);
    Duration::from_millis(millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn classifies_status() {
        let h = HeaderMap::new();
        assert!(matches!(from_status(StatusCode::SERVICE_UNAVAILABLE, &h), Failure::Transient { .. }));
        assert!(matches!(from_status(StatusCode::TOO_MANY_REQUESTS, &h), Failure::Transient { .. }));
        assert!(matches!(from_status(StatusCode::NOT_FOUND, &h), Failure::Permanent(_)));
        assert!(matches!(from_status(StatusCode::FORBIDDEN, &h), Failure::Permanent(_)));
    }

    #[test]
    fn retry_after_seconds_and_cap() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(7)));
        h.insert(RETRY_AFTER, HeaderValue::from_static("99999"));
        assert_eq!(parse_retry_after(&h), Some(MAX_RETRY_AFTER));
        h.insert(RETRY_AFTER, HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"));
        assert_eq!(parse_retry_after(&h), Some(Duration::ZERO));
    }

    #[test]
    fn backoff_bounds() {
        let cap = Duration::from_secs(60);
        for attempt in 0..20 {
            let d = backoff_delay(attempt, cap);
            assert!(d >= Duration::from_millis(500) && d <= cap, "{d:?}");
        }
    }
}
