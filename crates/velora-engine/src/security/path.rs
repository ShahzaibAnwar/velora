use std::path::{Component, Path, PathBuf};

use crate::model::ErrorInfo;

/// Validate a destination folder chosen by the user: it must be an absolute local path without `..`
/// components. The folder is created if it does not exist yet.
pub fn validate_dest_dir(dir: &str) -> Result<PathBuf, ErrorInfo> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err(ErrorInfo::new("path", "Choose a folder to save the file in."));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err(ErrorInfo::new("path", "The download folder must be a full path such as C:\\Users\\you\\Downloads."));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ErrorInfo::new("path", "The download folder may not contain '..'."));
    }
    Ok(path)
}

/// Join a sanitised file name onto a folder and make sure the result stays directly inside it.
pub fn confine(dir: &Path, file_name: &str) -> Result<PathBuf, ErrorInfo> {
    let candidate = dir.join(file_name);
    let mut comps = candidate.strip_prefix(dir).map_err(|_| escape_error())?.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(candidate),
        _ => Err(escape_error()),
    }
}

fn escape_error() -> ErrorInfo {
    ErrorInfo::new("path", "The file name tried to leave the download folder.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confine_rejects_escape() {
        let dir = std::env::temp_dir();
        assert!(confine(&dir, "ok.zip").is_ok());
        assert!(confine(&dir, "../x.zip").is_err());
        assert!(confine(&dir, "a/b.zip").is_err());
        assert!(confine(&dir, "").is_err());
    }

    #[test]
    fn dest_dir_rules() {
        assert!(validate_dest_dir("relative\\dir").is_err());
        assert!(validate_dest_dir("").is_err());
        let abs = std::env::temp_dir();
        assert!(validate_dest_dir(abs.to_str().unwrap()).is_ok());
        let up = abs.join("..").join("x");
        assert!(validate_dest_dir(up.to_str().unwrap()).is_err());
    }
}
