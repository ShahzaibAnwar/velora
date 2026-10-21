/// Reduce a URL to something safe to log: scheme, host, port and path. User info, query strings and
/// fragments (which often carry tokens or signatures) are removed.
pub fn redact_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(mut u) => {
            let had_query = u.query().is_some();
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.set_query(None);
            u.set_fragment(None);
            let mut s = u.to_string();
            if had_query {
                s.push_str("?…");
            }
            s
        }
        Err(_) => "<invalid url>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_secrets() {
        assert_eq!(redact_url("https://user:pass@example.com/f.zip?sig=abc#x"), "https://example.com/f.zip?…");
        assert_eq!(redact_url("http://example.com/a"), "http://example.com/a");
        assert_eq!(redact_url("nonsense"), "<invalid url>");
    }
}
