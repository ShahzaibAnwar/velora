//! Forward engine events to the webview and drive OS integration (notifications, taskbar, tray).
//!
//! The sink runs on engine threads and must never call back into the engine synchronously.

use std::sync::{Arc, Mutex, RwLock};

use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use velora_engine::{EngineEvent, EventSink, ProgressUpdate, Settings};

pub const ENGINE_EVENT: &str = "engine://event";

pub fn make_sink(app: AppHandle, prefs: Arc<RwLock<Settings>>) -> EventSink {
    let last_taskbar: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    Arc::new(move |event: EngineEvent| {
        match &event {
            EngineEvent::Progress { items, total_speed } => {
                crate::system::keep_awake::set(!items.is_empty());
                update_taskbar(&app, items, &last_taskbar);
                update_tray_tooltip(&app, items.len(), *total_speed);
            }
            EngineEvent::Completed { record } => {
                if prefs.read().unwrap().notify_on_complete {
                    notify(&app, "Download complete", &record.file_name);
                }
            }
            EngineEvent::Failed { record } => {
                if prefs.read().unwrap().notify_on_failure {
                    let reason = record.error.as_ref().map(|e| e.message.as_str()).unwrap_or("Unknown error");
                    notify(&app, "Download failed", &format!("{} — {}", record.file_name, reason));
                }
            }
            _ => {}
        }
        let _ = app.emit(ENGINE_EVENT, &event);
    })
}

pub(crate) fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!(error = %e, "notification failed");
    }
}

/// Aggregate progress of downloads with a known size into the taskbar button.
fn update_taskbar(app: &AppHandle, items: &[ProgressUpdate], last: &Mutex<Option<u64>>) {
    let Some(window) = app.get_webview_window("main") else { return };
    let (done, total) =
        items.iter().filter_map(|p| p.total_size.map(|t| (p.downloaded.min(t), t))).fold((0u64, 0u64), |(d, t), (pd, pt)| (d + pd, t + pt));
    let next = if items.is_empty() {
        None
    } else if total > 0 {
        Some(done * 100 / total)
    } else {
        Some(0)
    };
    let mut last = last.lock().unwrap();
    if *last == next {
        return;
    }
    *last = next;
    let state = match next {
        None => ProgressBarState { status: Some(ProgressBarStatus::None), progress: None },
        Some(_) if total == 0 => ProgressBarState { status: Some(ProgressBarStatus::Indeterminate), progress: None },
        Some(p) => ProgressBarState { status: Some(ProgressBarStatus::Normal), progress: Some(p) },
    };
    let _ = window.set_progress_bar(state);
}

fn update_tray_tooltip(app: &AppHandle, active: usize, speed: u64) {
    if let Some(tray) = app.tray_by_id(crate::system::tray::TRAY_ID) {
        let text = if active == 0 {
            "Velora".to_string()
        } else {
            format!("Velora — {active} active · {}/s", velora_engine::util::format_bytes(speed))
        };
        let _ = tray.set_tooltip(Some(text));
    }
}
