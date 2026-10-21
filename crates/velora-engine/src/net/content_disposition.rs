//! `Content-Disposition` file name extraction (RFC 6266 with RFC 8187 extended parameters).

use percent_encoding::percent_decode;

/// Return the file name a server suggests, preferring `filename*` over `filename`. The result is
/// *not* sanitised; pass it through `sanitize_file_name`.
pub fn file_name(header: &[u8]) -> Option<String> {
    let value = String::from_utf8_lossy(header);
    let params = split_params(&value);

    let extended = params.iter().find(|(k, _)| k.eq_ignore_ascii_case("filename*")).and_then(|(_, v)| decode_ext_value(v));
    if let Some(name) = extended.filter(|n| !n.trim().is_empty()) {
        return Some(name);
    }

    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("filename"))
        .map(|(_, v)| {
            // Many servers percent-encode plain `filename` values; decode when the result is valid UTF-8.
            if v.contains('%') {
                if let Ok(decoded) = percent_decode(v.as_bytes()).decode_utf8() {
                    return decoded.into_owned();
                }
            }
            v.clone()
        })
        .filter(|n| !n.trim().is_empty())
}

/// Split `attachment; a=b; c="d;e"` into `[(a, b), (c, d;e)]`, unescaping quoted strings.
fn split_params(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = value.chars().peekable();
    // Skip the disposition type.
    for c in chars.by_ref() {
        if c == ';' {
            break;
        }
    }
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace() || *c == ';') {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c == ';' {
                break;
            }
            key.push(c);
            chars.next();
        }
        if chars.peek().is_none() && key.trim().is_empty() {
            break;
        }
        let mut val = String::new();
        if chars.peek() == Some(&'=') {
            chars.next();
            while chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            if chars.peek() == Some(&'"') {
                chars.next();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                val.push(n);
                            }
                        }
                        '"' => break,
                        c => val.push(c),
                    }
                }
                // Discard anything up to the next separator.
                while chars.peek().is_some_and(|c| *c != ';') {
                    chars.next();
                }
            } else {
                while let Some(&c) = chars.peek() {
                    if c == ';' {
                        break;
                    }
                    val.push(c);
                    chars.next();
                }
                val = val.trim().to_string();
            }
        }
        let key = key.trim().to_string();
        if !key.is_empty() {
            out.push((key, val));
        }
        if chars.peek().is_none() {
            break;
        }
    }
    out
}

/// Decode an RFC 8187 `charset'lang'value`.
fn decode_ext_value(v: &str) -> Option<String> {
    let mut parts = v.splitn(3, '\'');
    let charset = parts.next()?.trim().to_ascii_lowercase();
    let _lang = parts.next()?;
    let encoded = parts.next()?;
    let bytes: Vec<u8> = percent_decode(encoded.as_bytes()).collect();
    match charset.as_str() {
        "utf-8" | "utf8" => Some(String::from_utf8_lossy(&bytes).into_owned()),
        "iso-8859-1" | "latin1" => Some(bytes.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_quoted() {
        assert_eq!(file_name(b"attachment; filename=report.pdf").as_deref(), Some("report.pdf"));
        assert_eq!(file_name(b"attachment; filename=\"my file; v2.zip\"").as_deref(), Some("my file; v2.zip"));
        assert_eq!(file_name(b"attachment; filename=\"a\\\"b.txt\"").as_deref(), Some("a\"b.txt"));
        assert_eq!(file_name(b"inline").as_deref(), None);
    }

    #[test]
    fn extended_wins() {
        let h = "attachment; filename=\"fallback.txt\"; filename*=UTF-8''%E2%82%AC%20rates.txt";
        assert_eq!(file_name(h.as_bytes()).as_deref(), Some("€ rates.txt"));
        let latin = "attachment; filename*=iso-8859-1'en'%A3%20rates.txt";
        assert_eq!(file_name(latin.as_bytes()).as_deref(), Some("£ rates.txt"));
    }

    #[test]
    fn percent_encoded_plain() {
        assert_eq!(file_name(b"attachment; filename=my%20video.mp4").as_deref(), Some("my video.mp4"));
    }
}
