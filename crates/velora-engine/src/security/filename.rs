use std::path::Path;

const MAX_NAME_CHARS: usize = 200;
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Turn an untrusted name (from a server, URL or user) into a safe single path component for
/// Windows. Never returns an empty string, `.`/`..`, a reserved device name, or anything containing a
/// path separator.
pub fn sanitize_file_name(raw: &str) -> String {
    // Keep only the last path component, whatever separator the server used.
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw);

    let mut out = String::with_capacity(last.len());
    for c in last.chars() {
        match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push('_'),
            c if c.is_control() => {}
            // Bidirectional overrides can disguise extensions ("gnp.exe" shown as "exe.png").
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' => {}
            c => out.push(c),
        }
    }

    let mut name = out.trim().trim_end_matches(['.', ' ']).trim_start_matches(' ').to_string();
    if name.is_empty() || name.chars().all(|c| c == '.') {
        name = "download".to_string();
    }

    let stem_upper = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    if RESERVED.contains(&stem_upper.as_str()) {
        name.insert(0, '_');
    }

    truncate_preserving_extension(&name, MAX_NAME_CHARS)
}

fn truncate_preserving_extension(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    let ext_len = ext.map_or(0, |e| e.chars().count() + 1);
    if ext_len >= max_chars / 2 {
        return name.chars().take(max_chars).collect();
    }
    let stem: String = stem.chars().take(max_chars - ext_len).collect();
    match ext {
        Some(e) => format!("{}.{}", stem.trim_end(), e),
        None => stem,
    }
}

/// Split `archive.tar.gz` into (`archive.tar`, `gz`). Names starting with a dot have no extension.
pub fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}

/// Return `name` if it is free in `dir` (and not in `taken`), otherwise `stem (n).ext`.
pub fn unique_file_name(dir: &Path, name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let is_free = |candidate: &str| !dir.join(candidate).exists() && !taken(candidate);
    if is_free(name) {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    for n in 1..10_000 {
        let candidate = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        if is_free(&candidate) {
            return candidate;
        }
    }
    format!("{stem}-{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_traversal_and_separators() {
        assert_eq!(sanitize_file_name("../../Windows/System32/evil.dll"), "evil.dll");
        assert_eq!(sanitize_file_name("..\\..\\Startup\\run.bat"), "run.bat");
        assert_eq!(sanitize_file_name(".."), "download");
        assert_eq!(sanitize_file_name(""), "download");
        assert_eq!(sanitize_file_name("   "), "download");
    }

    #[test]
    fn replaces_reserved_characters_and_names() {
        assert_eq!(sanitize_file_name("a<b>c:d\"e|f?g*h.txt"), "a_b_c_d_e_f_g_h.txt");
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_file_name("com1.tar.gz"), "_com1.tar.gz");
        assert_eq!(sanitize_file_name("console.txt"), "console.txt");
        assert_eq!(sanitize_file_name("report. . "), "report");
        assert_eq!(sanitize_file_name("tab\there\n.txt"), "tabhere.txt");
    }

    #[test]
    fn removes_bidi_overrides() {
        assert_eq!(sanitize_file_name("photo\u{202E}gnp.exe"), "photognp.exe");
    }

    #[test]
    fn truncates_but_keeps_extension() {
        let long = format!("{}.iso", "x".repeat(400));
        let out = sanitize_file_name(&long);
        assert!(out.chars().count() <= MAX_NAME_CHARS);
        assert!(out.ends_with(".iso"));
    }

    #[test]
    fn unique_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), b"x").unwrap();
        std::fs::write(dir.path().join("a (1).zip"), b"x").unwrap();
        assert_eq!(unique_file_name(dir.path(), "a.zip", &|_| false), "a (2).zip");
        assert_eq!(unique_file_name(dir.path(), "b.zip", &|n| n == "b.zip"), "b (1).zip");
        assert_eq!(unique_file_name(dir.path(), "README", &|_| false), "README");
    }
}
