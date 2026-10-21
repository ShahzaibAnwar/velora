//! The app side of the browser bridge: a local named-pipe server.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::oneshot;
use velora_engine::{AddRequest, StartMode};

use super::protocol::*;
use crate::state::AppState;

pub struct BridgeState {
    token: String,
    pending: Mutex<HashMap<String, Vec<oneshot::Sender<bool>>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub origin: String,
    pub browser: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAdd {
    pub url: String,
    pub referrer: Option<String>,
    pub filename: Option<String>,
    pub page_title: Option<String>,
}

impl BridgeState {
    /// Load the bridge token, creating it (32 random bytes) on first run.
    pub fn load_or_create(token_file: &Path) -> io::Result<Self> {
        let token = match std::fs::read_to_string(token_file) {
            Ok(t) if t.trim().len() == 64 => t.trim().to_string(),
            _ => {
                use rand::RngCore;
                let mut bytes = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                if let Some(dir) = token_file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let tmp = token_file.with_extension("tmp");
                std::fs::write(&tmp, &token)?;
                std::fs::rename(&tmp, token_file)?;
                token
            }
        };
        Ok(Self { token, pending: Mutex::new(HashMap::new()) })
    }

    /// Resolve every request waiting on `origin`.
    pub fn decide(&self, origin: &str, allow: bool) {
        if let Some(waiters) = self.pending.lock().unwrap().remove(origin) {
            for w in waiters {
                let _ = w.send(allow);
            }
        }
    }
}

pub fn browser_label(origin: &str) -> String {
    if origin.starts_with("moz-extension:") {
        "Firefox".into()
    } else if origin.starts_with("chrome-extension://") {
        "Chrome or Edge".into()
    } else {
        "a browser".into()
    }
}

pub fn start(app: AppHandle, state: Arc<BridgeState>) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app, state).await {
            // Most likely another Velora instance (or a squatter) owns the pipe name.
            tracing::error!(error = %e, "browser bridge stopped");
        }
    });
}

async fn serve(app: AppHandle, state: Arc<BridgeState>) -> io::Result<()> {
    let name = pipe_name();
    // `first_pipe_instance` fails if anyone else already created this pipe, so we never share a
    // name with an impostor; `reject_remote_clients` keeps it local to this machine.
    let mut server = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&name)?;
    tracing::info!("browser bridge listening");
    loop {
        server.connect().await?;
        let next = ServerOptions::new().reject_remote_clients(true).create(&name)?;
        let client = std::mem::replace(&mut server, next);
        let (app, state) = (app.clone(), state.clone());
        tauri::async_runtime::spawn(async move {
            if let Err(e) = handle(client, app, state).await {
                tracing::debug!(error = %e, "bridge client error");
            }
        });
    }
}

async fn handle(mut pipe: NamedPipeServer, app: AppHandle, state: Arc<BridgeState>) -> io::Result<()> {
    let mut len = [0u8; 4];
    tokio::time::timeout(Duration::from_secs(10), pipe.read_exact(&mut len)).await.map_err(|_| io::ErrorKind::TimedOut)??;
    let len = u32::from_le_bytes(len);
    if len > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut buf = vec![0u8; len as usize];
    pipe.read_exact(&mut buf).await?;
    let response = match serde_json::from_slice::<BridgeRequest>(&buf) {
        Ok(req) => process(req, &app, &state).await,
        Err(_) => BridgeResponse::error("Malformed request."),
    };
    let payload = serde_json::to_vec(&response)?;
    pipe.write_all(&(payload.len() as u32).to_le_bytes()).await?;
    pipe.write_all(&payload).await?;
    pipe.flush().await?;
    Ok(())
}

async fn process(req: BridgeRequest, app: &AppHandle, state: &BridgeState) -> BridgeResponse {
    if !tokens_equal(&req.token, &state.token) {
        tracing::warn!("rejected bridge request with a bad token");
        return BridgeResponse::error("Not authorised.");
    }
    let engine = app.state::<AppState>().engine.clone();
    let approved = engine.settings().approved_origins.iter().any(|o| o == &req.origin);
    let version = Some(env!("CARGO_PKG_VERSION").to_string());
    match req.message {
        ExtensionMessage::Ping => BridgeResponse { ok: true, version, approved: Some(approved), ..Default::default() },
        ExtensionMessage::Status => {
            BridgeResponse { ok: true, version, approved: Some(approved), active: Some(engine.stats().active), ..Default::default() }
        }
        ExtensionMessage::Add { url, referrer, filename, page_title, .. } => {
            if !approved && !request_approval(app, state, &req.origin).await {
                return BridgeResponse::error(
                    "Velora didn't allow this browser extension. You can approve it in Velora's Browser Integration page.",
                );
            }
            if let Err(e) = velora_engine::security::parse_download_url(&url) {
                return BridgeResponse::error(e.message);
            }
            if engine.settings().show_add_dialog_for_browser {
                crate::show_main_window(app);
                let _ = app.emit("bridge://add", BrowserAdd { url, referrer, filename, page_title });
                BridgeResponse { ok: true, version, ..Default::default() }
            } else {
                // A page with a known media resolver (e.g. an X post) downloads its best video file,
                // not the page itself.
                let (url, referrer, filename) = match engine.resolve_media(&url).await {
                    Ok(Some(media)) => {
                        let best = &media.variants[0];
                        (best.url.clone(), Some(media.page_url.clone()), Some(best.file_name.clone()))
                    }
                    Ok(None) => {
                        // Never silently save a web page when the browser meant to send a file.
                        if let Ok(info) = engine.probe(&url, referrer.as_deref()).await {
                            if info.mime.as_deref().is_some_and(|m| m.to_ascii_lowercase().starts_with("text/html")) {
                                return BridgeResponse::error("That link opens a web page, not a downloadable file.");
                            }
                        }
                        (url, referrer, filename)
                    }
                    Err(e) => return BridgeResponse::error(e.message),
                };
                let request = AddRequest { url, referrer, file_name: filename, start: StartMode::Now, ..Default::default() };
                match engine.add(request) {
                    Ok(rec) => BridgeResponse { ok: true, version, download_id: Some(rec.id), ..Default::default() },
                    Err(e) => BridgeResponse::error(e.message),
                }
            }
        }
    }
}

/// Ask the user (in the app) whether `origin` may add downloads. Waits up to two minutes.
async fn request_approval(app: &AppHandle, state: &BridgeState, origin: &str) -> bool {
    let (tx, rx) = oneshot::channel();
    state.pending.lock().unwrap().entry(origin.to_string()).or_default().push(tx);
    crate::show_main_window(app);
    let _ = app.emit("bridge://approval", ApprovalRequest { origin: origin.to_string(), browser: browser_label(origin) });
    matches!(tokio::time::timeout(Duration::from_secs(120), rx).await, Ok(Ok(true)))
}
