//! Crash-safe checkpoint written next to the partial file.
//!
//! The checkpoint is replaced atomically (write to a temp file, flush, rename) and is only ever
//! written *after* the part file has been flushed, so it can under-report progress after a crash but
//! never over-report it.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::segment::SegmentState;
use crate::model::ProbeInfo;

const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub version: u32,
    pub id: String,
    pub url: String,
    pub final_url: String,
    pub size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub segments: Vec<SegmentState>,
}

impl Checkpoint {
    pub fn new(id: &str, info: &ProbeInfo, segments: Vec<SegmentState>) -> Self {
        Self {
            version: VERSION,
            id: id.to_string(),
            url: info.url.clone(),
            final_url: info.final_url.clone(),
            size: info.size.unwrap_or(0),
            etag: info.etag.clone(),
            last_modified: info.last_modified.clone(),
            segments,
        }
    }

    /// Does the remote resource described by `info` still look like the one we started with?
    pub fn matches_remote(&self, info: &ProbeInfo) -> bool {
        if info.size != Some(self.size) {
            return false;
        }
        match (&self.etag, &info.etag) {
            (Some(a), Some(b)) => return a == b,
            (Some(_), None) | (None, Some(_)) => return false,
            (None, None) => {}
        }
        match (&self.last_modified, &info.last_modified) {
            (Some(a), Some(b)) => a == b,
            (Some(_), None) | (None, Some(_)) => false,
            (None, None) => true,
        }
    }

    /// Segments must be sorted, contiguous, cover `[0, size)` and not claim more than they hold.
    pub fn is_consistent(&self) -> bool {
        if self.version != VERSION || self.size == 0 || self.segments.is_empty() {
            return false;
        }
        let mut expected = 0u64;
        for s in &self.segments {
            if s.start != expected || s.end < s.start || s.written > s.end - s.start + 1 {
                return false;
            }
            expected = s.end + 1;
        }
        expected == self.size
    }

    pub fn written(&self) -> u64 {
        self.segments.iter().map(|s| s.written).sum()
    }
}

pub fn checkpoint_path(target: &Path) -> PathBuf {
    with_suffix(target, ".velora.json")
}

pub fn part_path(target: &Path) -> PathBuf {
    with_suffix(target, ".velora-part")
}

fn with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut s = target.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

pub fn load(path: &Path) -> Option<Checkpoint> {
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub fn save(path: &Path, ck: &Checkpoint) -> io::Result<()> {
    let tmp = with_suffix(path, ".tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(ck).map_err(io::Error::other)?)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

pub async fn save_async(path: PathBuf, ck: Checkpoint) -> io::Result<()> {
    tokio::task::spawn_blocking(move || save(&path, &ck)).await.map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(segments: Vec<SegmentState>, size: u64) -> Checkpoint {
        Checkpoint {
            version: VERSION,
            id: "x".into(),
            url: "u".into(),
            final_url: "u".into(),
            size,
            etag: Some("\"a\"".into()),
            last_modified: None,
            segments,
        }
    }

    #[test]
    fn consistency() {
        let ok = ck(vec![SegmentState { start: 0, end: 9, written: 10 }, SegmentState { start: 10, end: 19, written: 3 }], 20);
        assert!(ok.is_consistent());
        let gap = ck(vec![SegmentState { start: 0, end: 9, written: 0 }, SegmentState { start: 11, end: 19, written: 0 }], 20);
        assert!(!gap.is_consistent());
        let over = ck(vec![SegmentState { start: 0, end: 9, written: 11 }], 10);
        assert!(!over.is_consistent());
    }

    #[test]
    fn roundtrip_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.velora.json");
        let c = ck(vec![SegmentState { start: 0, end: 9, written: 4 }], 10);
        save(&p, &c).unwrap();
        save(&p, &c).unwrap();
        assert_eq!(load(&p), Some(c));
    }
}
