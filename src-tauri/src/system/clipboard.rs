//! Opt-in clipboard monitoring.
//!
//! Polls the clipboard *sequence number* (a cheap counter) and only reads text when it changes. Text
//! that looks like a link to a downloadable file is offered to the UI once; nothing is stored beyond a
//! small in-memory list used to avoid offering the same link twice, and nothing leaves the machine.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub struct ClipboardWatcher {
    enabled: AtomicBool,
}

#[derive(Clone, Serialize)]
pub struct ClipboardLink {
    pub url: String,
}

impl ClipboardWatcher {
    pub fn new(enabled: bool) -> Self {
        Self { enabled: AtomicBool::new(enabled) }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }
}

#[cfg(windows)]
fn sequence_number() -> u32 {
    // SAFETY: no arguments; returns 0 when the clipboard is inaccessible.
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn sequence_number() -> u32 {
    0
}

pub fn start(app: AppHandle, watcher: Arc<ClipboardWatcher>) {
    tauri::async_runtime::spawn(async move {
        let mut last_seq = sequence_number();
        let mut was_enabled = watcher.enabled.load(Ordering::Acquire);
        let mut seen: VecDeque<String> = VecDeque::with_capacity(32);
        let mut interval = tokio::time::interval(Duration::from_millis(800));
        loop {
            interval.tick().await;
            let enabled = watcher.enabled.load(Ordering::Acquire);
            if !enabled {
                was_enabled = false;
                continue;
            }
            let seq = sequence_number();
            if !was_enabled {
                // Only react to copies made after monitoring was switched on.
                was_enabled = true;
                last_seq = seq;
                continue;
            }
            if seq == last_seq || cfg!(not(windows)) {
                continue;
            }
            last_seq = seq;
            let Ok(text) = app.clipboard().read_text() else { continue };
            let Some(url) = velora_engine::security::url::looks_like_file_link(&text) else { continue };
            let url = url.to_string();
            if seen.contains(&url) {
                continue;
            }
            if seen.len() == 32 {
                seen.pop_front();
            }
            seen.push_back(url.clone());
            let _ = app.emit("clipboard://link", ClipboardLink { url });
        }
    });
}
