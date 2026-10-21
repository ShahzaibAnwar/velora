//! `#[tauri::command]` handlers. They stay thin: validate, call the engine or OS, map errors to
//! `{ code, message }` (`ErrorInfo`), which the UI shows verbatim.

pub mod app;
pub mod browser;
pub mod downloads;

pub type CmdResult<T> = Result<T, velora_engine::ErrorInfo>;
