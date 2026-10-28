use tauri::State;
use tauri_plugin_opener::OpenerExt;
use velora_engine::{
    AddCheck, AddRequest, Category, ClearScope, DownloadDetails, DownloadRecord, DownloadStatus, ErrorInfo, Priority, ProbeInfo, Stats,
};

use super::CmdResult;
use crate::state::AppState;

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadRecord> {
    state.engine.list()
}

#[tauri::command]
pub fn get_details(state: State<'_, AppState>, id: String) -> Option<DownloadDetails> {
    state.engine.details(&id)
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Stats {
    state.engine.stats()
}

#[tauri::command]
pub async fn probe_url(state: State<'_, AppState>, url: String, referrer: Option<String>) -> CmdResult<ProbeInfo> {
    let engine = state.engine.clone();
    engine.probe(&url, referrer.as_deref()).await
}

#[tauri::command]
pub async fn resolve_media(state: State<'_, AppState>, url: String) -> CmdResult<Option<velora_engine::media::ResolvedMedia>> {
    let engine = state.engine.clone();
    engine.resolve_media(&url).await
}

/// The helper program (yt-dlp, ffmpeg) being installed, if any.
#[tauri::command]
pub fn get_tool_setup(state: State<'_, AppState>) -> Option<velora_engine::tools::ToolSetup> {
    state.engine.tool_setup()
}

#[tauri::command]
pub fn check_add(
    state: State<'_, AppState>,
    url: String,
    file_name: Option<String>,
    dest_dir: Option<String>,
    category: Option<Category>,
) -> CmdResult<AddCheck> {
    state.engine.check_add(&url, file_name.as_deref(), dest_dir.as_deref(), category)
}

#[tauri::command]
pub fn add_download(state: State<'_, AppState>, request: AddRequest) -> CmdResult<DownloadRecord> {
    state.engine.add(request)
}

#[tauri::command]
pub fn pause_download(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.engine.pause(&id)
}

#[tauri::command]
pub fn resume_download(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.engine.resume(&id)
}

#[tauri::command]
pub fn restart_download(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.engine.restart(&id)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.engine.cancel(&id)
}

#[tauri::command]
pub fn remove_downloads(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    state.engine.remove(&ids)
}

/// Move a finished file to the Recycle Bin and remove it from the list.
#[tauri::command]
pub fn delete_download_file(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let rec = state.engine.get(&id).ok_or_else(|| ErrorInfo::new("not_found", "That download no longer exists."))?;
    let path = rec.target_path();
    if rec.status == DownloadStatus::Completed && path.exists() {
        trash::delete(&path).map_err(|e| ErrorInfo::new("io", format!("Couldn't move the file to the Recycle Bin: {e}")))?;
    }
    state.engine.remove(&[id])
}

#[tauri::command]
pub fn clear_downloads(state: State<'_, AppState>, scope: ClearScope) -> CmdResult<Vec<String>> {
    state.engine.clear(scope)
}

#[tauri::command]
pub fn pause_all(state: State<'_, AppState>) {
    state.engine.pause_all();
}

#[tauri::command]
pub fn resume_all(state: State<'_, AppState>) {
    state.engine.resume_all();
}

#[tauri::command]
pub fn set_priority(state: State<'_, AppState>, id: String, priority: Priority) -> CmdResult<()> {
    state.engine.set_priority(&id, priority)
}

#[tauri::command]
pub fn reorder_queue(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    state.engine.reorder(&ids)
}

#[tauri::command]
pub fn set_speed_limit(state: State<'_, AppState>, id: String, limit: Option<u64>) -> CmdResult<()> {
    state.engine.set_speed_limit(&id, limit)
}

#[tauri::command]
pub fn set_connections(state: State<'_, AppState>, id: String, connections: u8) -> CmdResult<()> {
    state.engine.set_connections(&id, connections)
}

#[tauri::command]
pub fn schedule_download(state: State<'_, AppState>, id: String, at: Option<i64>) -> CmdResult<()> {
    state.engine.schedule(&id, at)
}

#[tauri::command]
pub fn rename_download(state: State<'_, AppState>, id: String, name: String) -> CmdResult<DownloadRecord> {
    state.engine.rename(&id, &name)
}

#[tauri::command]
pub fn set_category(state: State<'_, AppState>, id: String, category: Category) -> CmdResult<DownloadRecord> {
    state.engine.set_category(&id, category)
}

#[tauri::command]
pub async fn verify_checksum(state: State<'_, AppState>, id: String, expected: Option<String>) -> CmdResult<String> {
    let engine = state.engine.clone();
    engine.verify_checksum(&id, expected).await
}

#[tauri::command]
pub fn export_downloads(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    std::fs::write(&path, state.engine.export_json()).map_err(|e| ErrorInfo::new("io", format!("Couldn't save the export: {e}")))
}

#[tauri::command]
pub fn import_downloads(state: State<'_, AppState>, path: String) -> CmdResult<usize> {
    let meta = std::fs::metadata(&path).map_err(|e| ErrorInfo::new("io", format!("Couldn't read the file: {e}")))?;
    if meta.len() > 32 * 1024 * 1024 {
        return Err(ErrorInfo::new("import", "That file is too large to be a Velora export."));
    }
    let json = std::fs::read_to_string(&path).map_err(|e| ErrorInfo::new("io", format!("Couldn't read the file: {e}")))?;
    state.engine.import_json(&json)
}

/// Open a finished file with its default program. Only files Velora downloaded can be opened, and
/// the shell applies its usual security prompts.
#[tauri::command]
pub fn open_file(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let rec = state.engine.get(&id).ok_or_else(|| ErrorInfo::new("not_found", "That download no longer exists."))?;
    if rec.status != DownloadStatus::Completed {
        return Err(ErrorInfo::new("invalid_state", "The file isn't finished yet."));
    }
    let path = rec.target_path();
    if !path.exists() {
        return Err(ErrorInfo::new("missing", "The file was moved or deleted."));
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| ErrorInfo::new("open", format!("Couldn't open the file: {e}")))
}

#[tauri::command]
pub fn show_in_folder(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let rec = state.engine.get(&id).ok_or_else(|| ErrorInfo::new("not_found", "That download no longer exists."))?;
    let target = rec.target_path();
    let part = velora_engine::download::checkpoint::part_path(&target);
    let reveal = [target.clone(), part].into_iter().find(|p| p.exists());
    let result = match reveal {
        Some(p) => app.opener().reveal_item_in_dir(p),
        None => app.opener().open_path(rec.dest_dir.clone(), None::<&str>),
    };
    result.map_err(|e| ErrorInfo::new("open", format!("Couldn't open the folder: {e}")))
}
