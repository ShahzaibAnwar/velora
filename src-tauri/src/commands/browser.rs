use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use velora_engine::ErrorInfo;

use super::CmdResult;
use crate::state::AppState;
use crate::system::native_messaging::{self, NativeMessagingStatus};

#[tauri::command]
pub fn native_messaging_status(state: State<'_, AppState>) -> NativeMessagingStatus {
    native_messaging::status(&state.paths.data_dir)
}

#[tauri::command]
pub fn register_native_messaging(state: State<'_, AppState>) -> CmdResult<NativeMessagingStatus> {
    native_messaging::register(&state.paths.data_dir)
}

#[tauri::command]
pub fn unregister_native_messaging(state: State<'_, AppState>) -> CmdResult<NativeMessagingStatus> {
    native_messaging::unregister(&state.paths.data_dir)
}

/// The user's answer to "Allow this browser extension?".
#[tauri::command]
pub fn bridge_decide(state: State<'_, AppState>, origin: String, allow: bool) -> CmdResult<()> {
    if allow {
        let mut s = state.engine.settings();
        if !s.approved_origins.contains(&origin) {
            s.approved_origins.push(origin.clone());
            let saved = state.engine.update_settings(s)?;
            *state.prefs.write().unwrap() = saved;
        }
    }
    #[cfg(windows)]
    state.bridge.decide(&origin, allow);
    Ok(())
}

#[tauri::command]
pub fn revoke_origin(state: State<'_, AppState>, origin: String) -> CmdResult<()> {
    let mut s = state.engine.settings();
    s.approved_origins.retain(|o| o != &origin);
    let saved = state.engine.update_settings(s)?;
    *state.prefs.write().unwrap() = saved;
    Ok(())
}

/// Open the folder holding the unpacked extension so the user can load it in their browser.
#[tauri::command]
pub fn open_extension_folder(app: AppHandle, browser: String) -> CmdResult<()> {
    let sub = match browser.as_str() {
        "firefox" => "extensions/firefox",
        _ => "extensions/chromium",
    };
    let bundled = app.path().resource_dir().map(|d| d.join(sub)).ok().filter(|p| p.exists());
    // In development the extension sits in the repository.
    let dev = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(sub);
    let path =
        bundled.or_else(|| dev.exists().then_some(dev)).ok_or_else(|| ErrorInfo::new("missing", "The extension files weren't found."))?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| ErrorInfo::new("open", e.to_string()))
}
