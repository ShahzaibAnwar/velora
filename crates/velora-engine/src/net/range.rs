//! `Content-Range` parsing (RFC 9110 §14.4).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    /// Inclusive.
    pub end: u64,
    /// `None` when the server sent `*` for the complete length.
    pub total: Option<u64>,
}

/// Parse `bytes 0-499/1234` or `bytes 0-499/*`. Returns `None` for anything malformed, including
/// `end < start` or `end >= total`.
pub fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse::<u64>().ok()?),
    };
    if end < start || total.is_some_and(|t| end >= t) {
        return None;
    }
    Some(ContentRange { start, end, total })
}

/// Parse the complete length from an unsatisfied-range response: `bytes */1234`.
pub fn parse_unsatisfied_total(value: &str) -> Option<u64> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    rest.strip_prefix("*/")?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_content_range("bytes 0-0/100"), Some(ContentRange { start: 0, end: 0, total: Some(100) }));
        assert_eq!(parse_content_range("bytes 10-19/*"), Some(ContentRange { start: 10, end: 19, total: None }));
        assert_eq!(parse_content_range("bytes 5-4/100"), None);
        assert_eq!(parse_content_range("bytes 0-100/100"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
        assert_eq!(parse_content_range("bytes x-1/2"), None);
        assert_eq!(parse_unsatisfied_total("bytes */0"), Some(0));
    }
}
