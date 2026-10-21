//! File categorisation by extension, with MIME type as a fallback.

use crate::model::Category;
use crate::security::filename::split_ext;

const VIDEO: &[&str] = &["mp4", "mkv", "webm", "avi", "mov", "wmv", "flv", "m4v", "mpg", "mpeg", "3gp", "ts", "m2ts", "ogv"];
const AUDIO: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "ogg", "oga", "opus", "wma", "aiff", "alac", "mka"];
const IMAGE: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "svg", "heic", "avif", "ico", "psd", "raw"];
const DOCUMENT: &[&str] =
    &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "csv", "epub", "mobi", "md", "json", "xml"];
const ARCHIVE: &[&str] = &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "img", "cab", "lz", "lzma", "dmg"];
const APPLICATION: &[&str] =
    &["exe", "msi", "msix", "msixbundle", "appx", "apk", "aab", "deb", "rpm", "appimage", "pkg", "bat", "cmd", "jar"];

pub fn categorize(file_name: &str, mime: Option<&str>) -> Category {
    if let (_, Some(ext)) = split_ext(file_name) {
        if let Some(c) = by_extension(&ext.to_ascii_lowercase()) {
            return c;
        }
    }
    mime.map(by_mime).unwrap_or(Category::Other)
}

fn by_extension(ext: &str) -> Option<Category> {
    let table: [(&[&str], Category); 6] = [
        (VIDEO, Category::Video),
        (AUDIO, Category::Audio),
        (IMAGE, Category::Image),
        (DOCUMENT, Category::Document),
        (ARCHIVE, Category::Archive),
        (APPLICATION, Category::Application),
    ];
    table.iter().find(|(exts, _)| exts.contains(&ext)).map(|(_, c)| *c)
}

fn by_mime(mime: &str) -> Category {
    let essence = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match essence.split_once('/') {
        Some(("video", _)) => Category::Video,
        Some(("audio", _)) => Category::Audio,
        Some(("image", _)) => Category::Image,
        Some(("text", _)) => Category::Document,
        Some(("application", sub)) => match sub {
            "pdf" | "msword" | "rtf" | "epub+zip" => Category::Document,
            s if s.starts_with("vnd.openxmlformats") || s.starts_with("vnd.ms-") || s.starts_with("vnd.oasis") => Category::Document,
            "zip" | "x-zip-compressed" | "x-rar-compressed" | "vnd.rar" | "x-7z-compressed" | "gzip" | "x-gzip" | "x-tar" | "x-xz"
            | "x-bzip2" | "zstd" | "x-iso9660-image" => Category::Archive,
            "x-msdownload"
            | "x-msdos-program"
            | "x-msi"
            | "vnd.microsoft.portable-executable"
            | "vnd.android.package-archive"
            | "x-debian-package"
            | "x-rpm"
            | "java-archive" => Category::Application,
            _ => Category::Other,
        },
        _ => Category::Other,
    }
}

/// Extensions that make a copied link worth offering as a download.
pub fn is_known_download_extension(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    matches!(by_extension(&ext), Some(c) if c != Category::Image) && !matches!(ext.as_str(), "md" | "json" | "xml" | "txt" | "csv")
}

/// A sensible extension for a MIME type when the URL and headers give no file name extension.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    let essence = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "application/pdf" => "pdf",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/x-7z-compressed" => "7z",
        "application/vnd.rar" | "application/x-rar-compressed" => "rar",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-msdownload" | "application/vnd.microsoft.portable-executable" => "exe",
        "application/x-msi" => "msi",
        "application/vnd.android.package-archive" => "apk",
        "application/x-iso9660-image" => "iso",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/x-matroska" => "mkv",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/wav" | "audio/x-wav" => "wav",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "text/plain" => "txt",
        "text/html" => "html",
        "text/csv" => "csv",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorises() {
        assert_eq!(categorize("movie.MKV", None), Category::Video);
        assert_eq!(categorize("setup.exe", None), Category::Application);
        assert_eq!(categorize("data.tar.gz", None), Category::Archive);
        assert_eq!(categorize("download", Some("application/pdf")), Category::Document);
        assert_eq!(categorize("download", Some("video/mp4; codecs=avc1")), Category::Video);
        assert_eq!(categorize("download", None), Category::Other);
    }
}
