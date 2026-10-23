//! # velora-engine
//!
//! The download engine behind Velora Download Manager. It has no UI or Tauri dependency: the desktop
//! app drives it through [`Engine`] and receives [`EngineEvent`]s through a callback.
//!
//! Highlights:
//! * HTTP/HTTPS with redirects, streaming straight to disk.
//! * Multi-connection range downloads with work stealing, `If-Range` protection and strict
//!   `Content-Range` validation.
//! * Crash-safe checkpoints; resume after restart with remote revalidation.
//! * Retry with exponential backoff, jitter and `Retry-After`.
//! * Global, per-download and scheduled bandwidth limits.
//! * Persistent queue, scheduling, categories, duplicate detection, SHA-256 verification.
//! * Video pages and HLS/DASH streams through yt-dlp (fetched and verified on first use), with
//!   ffmpeg merging separate video and audio.
//! * BitTorrent magnet links and `.torrent` files through librqbit.

pub mod category;
pub mod download;
mod engine;
pub mod media;
pub mod model;
pub mod net;
pub mod process;
pub mod security;
pub mod settings;
pub mod storage;
pub mod tools;
pub mod torrent;
pub mod util;

pub use engine::{ClearScope, Engine, EngineConfig, EventSink};
pub use model::*;
pub use settings::Settings;
