use url::Url;

use crate::model::ErrorInfo;

pub const MAX_URL_LEN: usize = 8 * 1024;

/// Parse and validate a URL the user or browser wants to download. Only `http` and `https` with a
/// host are accepted.
pub fn parse_download_url(input: &str) -> Result<Url, ErrorInfo> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ErrorInfo::new("invalid_url", "Enter a download link."));
    }
    if trimmed.len() > MAX_URL_LEN {
        return Err(ErrorInfo::new("invalid_url", "This link is too long."));
    }
    let url = Url::parse(trimmed).map_err(|_| ErrorInfo::new("invalid_url", "This doesn't look like a valid web address."))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ErrorInfo::new("invalid_url", format!("Velora can download http:// and https:// links, not {other}:// links.")))
        }
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ErrorInfo::new("invalid_url", "The link has no host name."));
    }
    Ok(url)
}

/// Heuristic used by clipboard monitoring: does this text look like a link to a file?
pub fn looks_like_file_link(text: &str) -> Option<Url> {
    let text = text.trim();
    if text.contains(char::is_whitespace) {
        return None;
    }
    let url = parse_download_url(text).ok()?;
    let last = url.path_segments()?.next_back()?.to_ascii_lowercase();
    let (_, ext) = last.rsplit_once('.')?;
    crate::category::is_known_download_extension(ext).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_only() {
        assert!(parse_download_url("https://example.com/a.zip").is_ok());
        assert!(parse_download_url("  http://example.com/a.zip  ").is_ok());
        assert!(parse_download_url("file:///C:/Windows/win.ini").is_err());
        assert!(parse_download_url("javascript:alert(1)").is_err());
        assert!(parse_download_url("ftp://example.com/a").is_err());
        assert!(parse_download_url("\\\\server\\share\\a.zip").is_err());
        assert!(parse_download_url("").is_err());
    }

    #[test]
    fn file_link_heuristic() {
        assert!(looks_like_file_link("https://cdn.example.com/files/setup.exe").is_some());
        assert!(looks_like_file_link("https://example.com/video.MP4?token=1").is_some());
        assert!(looks_like_file_link("https://example.com/blog/post").is_none());
        assert!(looks_like_file_link("see https://example.com/a.zip").is_none());
    }
}
