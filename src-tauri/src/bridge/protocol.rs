//! Wire format shared by the native host, the app's pipe server and the extensions.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_IDENTIFIER: &str = "com.velora.downloadmanager";
/// Native-messaging host name registered with browsers.
pub const HOST_NAME: &str = "com.velora.download_manager";
/// IDs of the Chromium extension (fixed by the `key` in `extensions/chromium/manifest.json`).
pub const CHROMIUM_EXTENSION_IDS: &[&str] = &["ekimhgmgmpdokgffllechdpepeeakmhk"];
pub const FIREFOX_EXTENSION_ID: &str = "velora-extension@velora-download-manager";
/// Chrome caps messages to the host at 4 GB and from the host at 1 MB; we use a much smaller cap.
pub const MAX_MESSAGE: u32 = 256 * 1024;

/// Message from the extension, forwarded unchanged by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ExtensionMessage {
    Ping,
    Status,
    Add {
        url: String,
        #[serde(default)]
        referrer: Option<String>,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default)]
        page_title: Option<String>,
        /// `context-menu`, `popup` or `handoff`.
        #[serde(default)]
        source: Option<String>,
    },
}

/// What the host sends to the app over the pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
    pub token: String,
    /// `chrome-extension://<id>/` or `moz-extension:<id>`; as reported by the browser to the host.
    pub origin: String,
    pub message: ExtensionMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<u32>,
}

impl BridgeResponse {
    pub fn error(message: impl Into<String>) -> Self {
        Self { ok: false, error: Some(message.into()), ..Default::default() }
    }
}

/// Native messaging framing: 4-byte little-endian length, then UTF-8 JSON.
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

pub fn pipe_name() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    let safe: String = user.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!(r"\\.\pipe\velora-bridge-{safe}")
}

/// `%APPDATA%\com.velora.downloadmanager\bridge.token` — the same folder Tauri uses for app data.
pub fn token_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(APP_IDENTIFIER).join("bridge.token"))
}

/// Constant-time comparison so the token can't be guessed byte by byte from timing.
pub fn tokens_equal(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, br#"{"type":"ping"}"#).unwrap();
        let mut r = &buf[..];
        let got = read_frame(&mut r).unwrap().unwrap();
        let msg: ExtensionMessage = serde_json::from_slice(&got).unwrap();
        assert!(matches!(msg, ExtensionMessage::Ping));
        assert!(read_frame(&mut r).unwrap().is_none());
    }

    #[test]
    fn rejects_oversized_frames() {
        let mut buf = (MAX_MESSAGE + 1).to_le_bytes().to_vec();
        buf.extend_from_slice(b"x");
        assert!(read_frame(&mut &buf[..]).is_err());
    }

    #[test]
    fn token_compare() {
        assert!(tokens_equal("abc", "abc"));
        assert!(!tokens_equal("abc", "abd"));
        assert!(!tokens_equal("abc", "ab"));
    }

    #[test]
    fn parses_add() {
        let m: ExtensionMessage =
            serde_json::from_str(r#"{"type":"add","url":"https://e.com/a.zip","pageTitle":"x","source":"context-menu"}"#).unwrap();
        assert!(matches!(m, ExtensionMessage::Add { .. }));
    }
}
