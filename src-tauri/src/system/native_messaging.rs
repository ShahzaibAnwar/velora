//! Register Velora as a native-messaging host for Chrome, Edge (and other Chromium browsers) and
//! Firefox. Only done when the user asks for it in Settings; everything lives under HKCU and the
//! user's app-data folder and is removed again by `unregister`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use velora_engine::ErrorInfo;

use crate::bridge::protocol::{CHROMIUM_EXTENSION_IDS, FIREFOX_EXTENSION_ID, HOST_NAME};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub id: String,
    pub name: String,
    pub registered: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeMessagingStatus {
    pub supported: bool,
    pub browsers: Vec<BrowserStatus>,
    pub host_path: String,
    pub manifest_dir: String,
    pub chromium_extension_ids: Vec<String>,
    pub firefox_extension_id: String,
}

struct Browser {
    id: &'static str,
    name: &'static str,
    key: &'static str,
    firefox: bool,
}

const BROWSERS: &[Browser] = &[
    Browser { id: "chrome", name: "Google Chrome", key: r"Software\Google\Chrome\NativeMessagingHosts", firefox: false },
    Browser { id: "edge", name: "Microsoft Edge", key: r"Software\Microsoft\Edge\NativeMessagingHosts", firefox: false },
    Browser { id: "chromium", name: "Chromium", key: r"Software\Chromium\NativeMessagingHosts", firefox: false },
    Browser { id: "firefox", name: "Mozilla Firefox", key: r"Software\Mozilla\NativeMessagingHosts", firefox: true },
];

fn manifest_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("native-messaging")
}

fn manifest_path(data_dir: &Path, firefox: bool) -> PathBuf {
    manifest_dir(data_dir).join(if firefox { "velora-host.firefox.json" } else { "velora-host.chromium.json" })
}

fn io_err(e: std::io::Error) -> ErrorInfo {
    ErrorInfo::new("native_messaging", format!("Couldn't update browser registration: {e}"))
}

pub fn register(data_dir: &Path) -> Result<NativeMessagingStatus, ErrorInfo> {
    let exe = std::env::current_exe().map_err(io_err)?;
    std::fs::create_dir_all(manifest_dir(data_dir)).map_err(io_err)?;

    let chromium = serde_json::json!({
        "name": HOST_NAME,
        "description": "Velora Download Manager",
        "path": exe,
        "type": "stdio",
        "allowed_origins": CHROMIUM_EXTENSION_IDS.iter().map(|id| format!("chrome-extension://{id}/")).collect::<Vec<_>>(),
    });
    let firefox = serde_json::json!({
        "name": HOST_NAME,
        "description": "Velora Download Manager",
        "path": exe,
        "type": "stdio",
        "allowed_extensions": [FIREFOX_EXTENSION_ID],
    });
    for (firefox_flag, value) in [(false, chromium), (true, firefox)] {
        let body = serde_json::to_vec_pretty(&value).map_err(|e| ErrorInfo::new("native_messaging", e.to_string()))?;
        std::fs::write(manifest_path(data_dir, firefox_flag), body).map_err(io_err)?;
    }

    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for b in BROWSERS {
            let (key, _) = hkcu.create_subkey(format!(r"{}\{}", b.key, HOST_NAME)).map_err(io_err)?;
            key.set_value("", &manifest_path(data_dir, b.firefox).to_string_lossy().into_owned()).map_err(io_err)?;
        }
    }
    Ok(status(data_dir))
}

pub fn unregister(data_dir: &Path) -> Result<NativeMessagingStatus, ErrorInfo> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for b in BROWSERS {
            match hkcu.delete_subkey_all(format!(r"{}\{}", b.key, HOST_NAME)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_err(e)),
            }
        }
    }
    for firefox in [false, true] {
        let _ = std::fs::remove_file(manifest_path(data_dir, firefox));
    }
    Ok(status(data_dir))
}

pub fn status(data_dir: &Path) -> NativeMessagingStatus {
    let exe = std::env::current_exe().unwrap_or_default();
    let browsers = BROWSERS
        .iter()
        .map(|b| BrowserStatus { id: b.id.into(), name: b.name.into(), registered: is_registered(data_dir, b, &exe) })
        .collect();
    NativeMessagingStatus {
        supported: cfg!(windows),
        browsers,
        host_path: exe.to_string_lossy().into_owned(),
        manifest_dir: manifest_dir(data_dir).to_string_lossy().into_owned(),
        chromium_extension_ids: CHROMIUM_EXTENSION_IDS.iter().map(|s| s.to_string()).collect(),
        firefox_extension_id: FIREFOX_EXTENSION_ID.into(),
    }
}

#[cfg(windows)]
fn is_registered(data_dir: &Path, b: &Browser, exe: &Path) -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(format!(r"{}\{}", b.key, HOST_NAME), KEY_READ) else { return false };
    let Ok(path): Result<String, _> = key.get_value("") else { return false };
    let expected = manifest_path(data_dir, b.firefox);
    if !velora_engine::util::same_path(Path::new(&path), &expected) {
        return false;
    }
    // The manifest must point at this executable (it changes when Velora is reinstalled elsewhere).
    std::fs::read(&expected)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .is_some_and(|p| velora_engine::util::same_path(&p, exe))
}

#[cfg(not(windows))]
fn is_registered(_data_dir: &Path, _b: &Browser, _exe: &Path) -> bool {
    false
}
