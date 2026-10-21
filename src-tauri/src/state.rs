use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use velora_engine::{Engine, Settings};

use crate::logging::LogControl;
use crate::system::clipboard::ClipboardWatcher;

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub db_path: PathBuf,
}

pub struct AppState {
    pub engine: Engine,
    /// Copy of the settings for code that must not call into the engine (the event sink).
    pub prefs: Arc<RwLock<Settings>>,
    pub paths: AppPaths,
    pub logs: LogControl,
    pub clipboard: Arc<ClipboardWatcher>,
    #[cfg(windows)]
    pub bridge: Arc<crate::bridge::server::BridgeState>,
    pub quitting: AtomicBool,
}
