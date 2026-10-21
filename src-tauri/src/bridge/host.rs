//! Native-messaging host mode: `velora.exe` started by a browser, talking JSON over stdio.
//!
//! The host is a thin, stateless relay. It authenticates to the running app with the per-user token
//! and forwards one request per message. It starts the app if a download is requested while Velora
//! is not running; pings never launch it.

use std::io::{self, Read};

use super::protocol::*;

pub fn is_native_host_invocation(args: &[String]) -> bool {
    let chrome = args.iter().skip(1).any(|a| a.starts_with("chrome-extension://"));
    let firefox = args.len() >= 3 && args[1].to_ascii_lowercase().ends_with(".json") && args[2] == FIREFOX_EXTENSION_ID;
    chrome || firefox || args.iter().any(|a| a == "--native-host")
}

fn origin_from_args(args: &[String]) -> String {
    if let Some(o) = args.iter().find(|a| a.starts_with("chrome-extension://")) {
        return o.clone();
    }
    if args.len() >= 3 && args[2] == FIREFOX_EXTENSION_ID {
        return format!("moz-extension:{FIREFOX_EXTENSION_ID}");
    }
    "unknown".to_string()
}

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    let origin = origin_from_args(&args);
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    loop {
        let frame = match read_frame(&mut stdin) {
            Ok(Some(f)) => f,
            Ok(None) => return 0,
            Err(_) => return 1,
        };
        let response = match serde_json::from_slice::<ExtensionMessage>(&frame) {
            Ok(msg) => forward(&origin, msg),
            Err(_) => BridgeResponse::error("Velora didn't understand the request."),
        };
        let payload = serde_json::to_vec(&response).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
        if write_frame(&mut stdout, &payload).is_err() {
            return 1;
        }
    }
}

fn forward(origin: &str, message: ExtensionMessage) -> BridgeResponse {
    let may_launch = matches!(message, ExtensionMessage::Add { .. });
    match relay(origin, message, may_launch) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => BridgeResponse::error("not_running"),
        Err(e) => BridgeResponse::error(format!("Couldn't reach Velora: {e}")),
    }
}

#[cfg(windows)]
fn relay(origin: &str, message: ExtensionMessage, may_launch: bool) -> io::Result<BridgeResponse> {
    use std::time::{Duration, Instant};

    let mut pipe = match connect() {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound && may_launch => {
            launch_app()?;
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                match connect() {
                    Ok(p) => break p,
                    Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(250)),
                    Err(e) => return Err(e),
                }
            }
        }
        Err(e) => return Err(e),
    };
    let token = read_token()?;
    let request = BridgeRequest { token, origin: origin.to_string(), message };
    write_frame(&mut pipe, &serde_json::to_vec(&request)?)?;
    let reply = read_frame(&mut pipe)?.ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no reply"))?;
    serde_json::from_slice(&reply).map_err(io::Error::other)
}

#[cfg(not(windows))]
fn relay(_origin: &str, _message: ExtensionMessage, _may_launch: bool) -> io::Result<BridgeResponse> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "browser integration is Windows-only in this release"))
}

fn read_token() -> io::Result<String> {
    let path = token_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA not set"))?;
    let mut s = String::new();
    std::fs::File::open(path)?.read_to_string(&mut s)?;
    Ok(s.trim().to_string())
}

/// Open the app's pipe, waiting briefly while all instances are busy.
#[cfg(windows)]
fn connect() -> io::Result<std::fs::File> {
    const ERROR_PIPE_BUSY: i32 = 231;
    let name = pipe_name();
    for _ in 0..40 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&name) {
            Ok(f) => return Ok(f),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "Velora is busy"))
}

/// Start the desktop app detached from the browser's process tree.
#[cfg(windows)]
fn launch_app() -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let exe = std::env::current_exe()?;
    let spawn = |flags: u32| {
        Command::new(&exe).arg("--minimized").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(flags).spawn()
    };
    spawn(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP))
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_invocations() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(is_native_host_invocation(&s(&["velora.exe", "chrome-extension://abc/", "--parent-window=0"])));
        assert!(is_native_host_invocation(&s(&["velora.exe", "C:\\x\\host.json", FIREFOX_EXTENSION_ID])));
        assert!(!is_native_host_invocation(&s(&["velora.exe"])));
        assert!(!is_native_host_invocation(&s(&["velora.exe", "--minimized"])));
    }
}
