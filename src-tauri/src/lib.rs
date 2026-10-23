//! Velora desktop app: wires the download engine to the webview UI and to Windows.

pub mod bridge;
mod commands;
mod events;
mod logging;
mod state;
mod system;

use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use velora_engine::{Engine, EngineConfig};

use crate::state::{AppPaths, AppState};
use crate::system::clipboard::ClipboardWatcher;

#[derive(Clone, Serialize)]
struct UrlArgs {
    urls: Vec<String>,
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Hide the window to the notification area. The first time ever, explain that Velora keeps running.
fn hide_to_tray(window: &tauri::Window, state: &AppState) {
    let _ = window.hide();
    let marker = state.paths.data_dir.join("tray-hint.shown");
    if !marker.exists() {
        let _ = std::fs::write(&marker, b"1");
        events::notify(
            window.app_handle(),
            "Velora is still running",
            "Downloads continue in the background. Use the Velora icon in the notification area to reopen or quit.",
        );
    }
}

fn save_window_state(app: &AppHandle) {
    if let (Some(w), Some(state)) = (app.get_webview_window("main"), app.try_state::<AppState>()) {
        system::window_state::save(&w, &state.paths.data_dir);
    }
}

/// Stop downloads cleanly (saving checkpoints), then exit.
pub fn request_quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.quitting.swap(true, Ordering::AcqRel) {
        return;
    }
    save_window_state(app);
    let engine = state.engine.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        engine.shutdown().await;
        app.exit(0);
    });
}

fn urls_from_args(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|a| a.starts_with("http://") || a.starts_with("https://") || a.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("magnet:")))
        .cloned()
        .collect()
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let launched_minimized = args.iter().any(|a| a == "--minimized");
    let initial_urls = urls_from_args(&args);
    system::window_icon::set_app_user_model_id(bridge::protocol::APP_IDENTIFIER);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main_window(app);
            let urls = urls_from_args(&argv);
            if !urls.is_empty() {
                let _ = app.emit("app://urls", UrlArgs { urls });
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(move |app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(&log_dir)?;
            let logs = logging::init(&log_dir, true);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "Velora starting");

            let download_dir = app.path().download_dir().or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))?;
            let db_path = data_dir.join("velora.db");
            let prefs = Arc::new(RwLock::new(velora_engine::Settings::default()));
            let sink = events::make_sink(handle.clone(), prefs.clone());
            let config = EngineConfig::new(Some(db_path.clone()), download_dir, sink).with_data_dir(data_dir.clone());
            let engine = tauri::async_runtime::block_on(Engine::open(config))
                .map_err(|e| format!("Velora couldn't open its database: {}", e.message))?;
            let settings = engine.settings();
            *prefs.write().unwrap() = settings.clone();
            logs.set_verbose(settings.diagnostic_logs);

            let clipboard = Arc::new(ClipboardWatcher::new(settings.clipboard_monitor));
            #[cfg(windows)]
            let bridge = Arc::new(
                bridge::server::BridgeState::load_or_create(&data_dir.join("bridge.token"))
                    .map_err(|e| format!("Couldn't create the browser bridge token: {e}"))?,
            );

            app.manage(AppState {
                engine,
                prefs,
                paths: AppPaths { data_dir, log_dir, db_path },
                logs,
                clipboard: clipboard.clone(),
                #[cfg(windows)]
                bridge: bridge.clone(),
                quitting: Default::default(),
            });

            #[cfg(windows)]
            bridge::server::start(handle.clone(), bridge);
            system::clipboard::start(handle.clone(), clipboard);
            system::tray::create(app)?;
            if let Err(e) = commands::app::apply_autostart(&handle, settings.launch_on_startup) {
                tracing::warn!(error = %e.message, "autostart sync failed");
            }

            if let Some(w) = app.get_webview_window("main") {
                system::window_state::restore(&w, &app.state::<AppState>().paths.data_dir);
                #[cfg(windows)]
                if let Ok(hwnd) = w.hwnd() {
                    system::window_icon::apply(hwnd.0 as isize);
                }
            }
            if !(launched_minimized || settings.start_minimized) {
                show_main_window(&handle);
            }
            if !initial_urls.is_empty() {
                let urls = initial_urls.clone();
                let h = handle.clone();
                // Give the UI a moment to subscribe before delivering URLs passed on the command line.
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    show_main_window(&h);
                    let _ = h.emit("app://urls", UrlArgs { urls });
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            let Some(state) = app.try_state::<AppState>() else { return };
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    save_window_state(app);
                    if state.prefs.read().unwrap().close_to_tray {
                        hide_to_tray(window, &state);
                    } else {
                        request_quit(app);
                    }
                }
                // Icon sizes depend on the monitor's scaling; reload them when it changes.
                #[cfg(windows)]
                WindowEvent::ScaleFactorChanged { .. } => {
                    if let Ok(hwnd) = window.hwnd() {
                        system::window_icon::apply(hwnd.0 as isize);
                    }
                }
                WindowEvent::Resized(_) => {
                    if state.prefs.read().unwrap().minimize_to_tray && window.is_minimized().unwrap_or(false) {
                        hide_to_tray(window, &state);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::downloads::list_downloads,
            commands::downloads::get_details,
            commands::downloads::get_stats,
            commands::downloads::probe_url,
            commands::downloads::resolve_media,
            commands::downloads::check_add,
            commands::downloads::add_download,
            commands::downloads::pause_download,
            commands::downloads::resume_download,
            commands::downloads::restart_download,
            commands::downloads::cancel_download,
            commands::downloads::remove_downloads,
            commands::downloads::delete_download_file,
            commands::downloads::clear_downloads,
            commands::downloads::pause_all,
            commands::downloads::resume_all,
            commands::downloads::set_priority,
            commands::downloads::reorder_queue,
            commands::downloads::set_speed_limit,
            commands::downloads::set_connections,
            commands::downloads::schedule_download,
            commands::downloads::rename_download,
            commands::downloads::set_category,
            commands::downloads::verify_checksum,
            commands::downloads::export_downloads,
            commands::downloads::import_downloads,
            commands::downloads::open_file,
            commands::downloads::show_in_folder,
            commands::app::app_info,
            commands::app::get_settings,
            commands::app::update_settings,
            commands::app::open_path,
            commands::app::open_external,
            commands::app::read_clipboard,
            commands::app::set_clipboard,
            commands::app::quit_app,
            commands::browser::native_messaging_status,
            commands::browser::register_native_messaging,
            commands::browser::unregister_native_messaging,
            commands::browser::bridge_decide,
            commands::browser::revoke_origin,
            commands::browser::open_extension_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Velora")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                let quitting = app.try_state::<AppState>().is_none_or(|s| s.quitting.load(Ordering::Acquire));
                if !quitting {
                    // Closing the last window should not kill downloads; quitting goes through `request_quit`.
                    api.prevent_exit();
                }
            }
        });
}
