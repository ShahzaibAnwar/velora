//! Data types shared between the engine, the storage layer and the UI.
//!
//! Every type here serialises as camelCase JSON. `src/types/engine.ts` in the frontend mirrors these
//! definitions; keep the two in sync.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Scheduled,
    Paused,
    Probing,
    Downloading,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    /// A task is running (or about to run) for this download.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Probing | Self::Downloading | Self::Verifying)
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Scheduled => "scheduled",
            Self::Paused => "paused",
            Self::Probing => "probing",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Video,
    Audio,
    Image,
    Document,
    Archive,
    Application,
    #[default]
    Other,
}

impl Category {
    pub const ALL: [Category; 7] =
        [Category::Video, Category::Audio, Category::Image, Category::Document, Category::Archive, Category::Application, Category::Other];

    /// Folder name used when automatic categorisation is on.
    pub fn folder_name(self) -> &'static str {
        match self {
            Category::Video => "Video",
            Category::Audio => "Music",
            Category::Image => "Images",
            Category::Document => "Documents",
            Category::Archive => "Compressed",
            Category::Application => "Programs",
            Category::Other => "Other",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Category::Video => "video",
            Category::Audio => "audio",
            Category::Image => "image",
            Category::Document => "document",
            Category::Archive => "archive",
            Category::Application => "application",
            Category::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ChecksumState {
    #[default]
    None,
    Pending,
    Verified,
    Mismatch,
}

/// A human-readable error with a stable machine code the UI can switch on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

/// The persisted state of one download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: String,
    /// The URL the user (or browser) supplied.
    pub url: String,
    /// The URL after redirects, from the most recent probe.
    #[serde(default)]
    pub final_url: Option<String>,
    pub file_name: String,
    /// `true` while the file name may still be replaced by the server-provided name.
    #[serde(default)]
    pub name_auto: bool,
    pub dest_dir: String,
    /// `true` when `dest_dir` was derived from the category and may follow a category change.
    #[serde(default)]
    pub dest_auto: bool,
    pub category: Category,
    pub status: DownloadStatus,
    #[serde(default)]
    pub priority: Priority,
    /// Position in the queue; lower runs first within the same priority.
    #[serde(default)]
    pub queue_pos: i64,
    #[serde(default)]
    pub total_size: Option<u64>,
    #[serde(default)]
    pub downloaded: u64,
    /// The server proved it honours byte ranges for this resource.
    #[serde(default)]
    pub resumable: bool,
    /// Requested maximum number of connections.
    pub connections: u8,
    /// Per-download bandwidth limit in bytes/s.
    #[serde(default)]
    pub speed_limit: Option<u64>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub error: Option<ErrorInfo>,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub scheduled_at: Option<i64>,
    /// Expected SHA-256 (lowercase hex), if the user supplied one.
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub checksum_state: ChecksumState,
    /// Replace an existing file with the same name when finishing.
    #[serde(default)]
    pub overwrite: bool,
    /// Discard any partial data on the next start instead of resuming.
    #[serde(default)]
    pub restart_pending: bool,
}

impl DownloadRecord {
    pub fn target_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.dest_dir).join(&self.file_name)
    }
}

/// How a new download should start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", tag = "mode", content = "at")]
pub enum StartMode {
    /// Start immediately, even if the queue is full.
    #[default]
    Now,
    /// Wait for a free slot in the queue.
    Queue,
    /// Add without starting.
    Paused,
    /// Start at the given Unix epoch milliseconds.
    Schedule(i64),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub dest_dir: Option<String>,
    #[serde(default)]
    pub category: Option<Category>,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default)]
    pub start: StartMode,
    #[serde(default)]
    pub connections: Option<u8>,
    #[serde(default)]
    pub speed_limit: Option<u64>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    /// Replace an existing file at the destination instead of choosing a new name.
    #[serde(default)]
    pub overwrite: bool,
}

/// What the server told us about a URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub url: String,
    pub final_url: String,
    pub file_name: String,
    pub host: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    /// Verified by a `206` response with a matching `Content-Range`.
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Non-identity `Content-Encoding`, which disables ranges.
    pub content_encoding: Option<String>,
    pub category: Category,
}

/// Result of checking a prospective download for duplicates and name conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCheck {
    /// An unfinished download with the same URL.
    pub duplicate_of: Option<DownloadRecord>,
    /// A file with this name already exists in the folder.
    pub file_exists: bool,
    /// Another unfinished download targets the same path.
    pub conflicts_with: Option<String>,
    pub dest_dir: String,
    pub file_name: String,
    /// A free alternative name in the same folder.
    pub suggested_name: String,
    pub category: Category,
}

/// Live, non-persisted numbers for a running download.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub id: String,
    pub downloaded: u64,
    pub total_size: Option<u64>,
    /// Bytes per second, smoothed.
    pub speed: u64,
    pub eta_secs: Option<u64>,
    pub connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentView {
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDetails {
    pub record: DownloadRecord,
    pub progress: Option<ProgressUpdate>,
    pub segments: Vec<SegmentView>,
    /// Bytes per second, one sample per second, oldest first.
    pub speed_history: Vec<u64>,
    pub part_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub active: u32,
    pub queued: u32,
    pub current_speed: u64,
    /// Average of the last 60 s of total speed, counting only seconds with activity.
    pub average_speed: u64,
    pub completed_today: u32,
    pub downloaded_today: u64,
    pub total_downloaded: u64,
    pub total_completed: u32,
    pub queue_paused: bool,
    pub effective_speed_limit: u64,
    /// Total speed, one sample per second, oldest first (up to 120 samples).
    pub speed_history: Vec<u64>,
}

/// Events the engine pushes to its host (the desktop app).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum EngineEvent {
    /// A record was added or changed (status, name, size, error…).
    Upserted {
        record: DownloadRecord,
    },
    Removed {
        ids: Vec<String>,
    },
    /// Periodic progress for running downloads (about twice a second).
    Progress {
        items: Vec<ProgressUpdate>,
        total_speed: u64,
    },
    /// A download finished successfully (also sent as `Upserted`).
    Completed {
        record: DownloadRecord,
    },
    /// A download failed (also sent as `Upserted`).
    Failed {
        record: DownloadRecord,
    },
    QueueState {
        paused: bool,
    },
}
