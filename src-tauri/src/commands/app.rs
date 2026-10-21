use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use velora_engine::{ErrorInfo, Settings};

use super::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub log_dir: String,
    pub db_path: String,
    pub default_download_dir: String,
    pub platform: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        log_dir: state.paths.log_dir.to_string_lossy().into_owned(),
        db_path: state.paths.db_path.to_string_lossy().into_owned(),
        default_download_dir: app.path().download_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.engine.settings()
}

/// Save settings and apply the parts that live outside the engine (autostart, logging, clipboard).
#[tauri::command]
pub fn update_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let saved = state.engine.update_settings(settings)?;
    *state.prefs.write().unwrap() = saved.clone();
    state.logs.set_verbose(saved.diagnostic_logs);
    state.clipboard.set_enabled(saved.clipboard_monitor);
    apply_autostart(&app, saved.launch_on_startup)?;
    Ok(saved)
}

pub fn apply_autostart(app: &AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    let current = launcher.is_enabled().unwrap_or(false);
    if current == enabled {
        return Ok(());
    }
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| ErrorInfo::new("autostart", format!("Couldn't change the startup setting: {e}")))
}

#[tauri::command]
pub fn open_path(app: AppHandle, state: State<'_, AppState>, which: String) -> CmdResult<()> {
    let path = match which.as_str() {
        "data" => state.paths.data_dir.clone(),
        "logs" => state.paths.log_dir.clone(),
        "downloads" => state.engine.settings().default_dir.into(),
        _ => return Err(ErrorInfo::new("invalid", "Unknown folder.")),
    };
    std::fs::create_dir_all(&path).ok();
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| ErrorInfo::new("open", e.to_string()))
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    // Only our own project links are opened from the UI.
    if !url.starts_with("https://github.com/") {
        return Err(ErrorInfo::new("invalid", "Only GitHub links can be opened from here."));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| ErrorInfo::new("open", e.to_string()))
}

/// Read text from the clipboard for the "Paste link" action (always user-initiated).
#[tauri::command]
pub fn read_clipboard(app: AppHandle) -> String {
    app.clipboard().read_text().unwrap_or_default()
}

#[tauri::command]
pub fn set_clipboard(app: AppHandle, text: String) -> CmdResult<()> {
    app.clipboard().write_text(text).map_err(|e| ErrorInfo::new("clipboard", e.to_string()))
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    crate::request_quit(&app);
}
