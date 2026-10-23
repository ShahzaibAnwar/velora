//! Keep Windows from sleeping while downloads are running (the display may still turn off).
//!
//! `SetThreadExecutionState` applies to the calling thread, and engine events arrive on whichever
//! runtime thread produced them, so one dedicated thread owns the request.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};

static SENDER: OnceLock<Mutex<Sender<bool>>> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Ask for the system to stay awake (`true`) or release the request (`false`). Cheap to call on
/// every progress tick: only changes are forwarded.
pub fn set(active: bool) {
    if ACTIVE.swap(active, Ordering::AcqRel) == active {
        return;
    }
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = channel::<bool>();
        let spawned = std::thread::Builder::new().name("velora-keep-awake".into()).spawn(move || {
            for awake in rx {
                apply(awake);
            }
            apply(false);
        });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "couldn't start the keep-awake thread");
        }
        Mutex::new(tx)
    });
    let _ = sender.lock().unwrap().send(active);
}

#[cfg(windows)]
fn apply(awake: bool) {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    let flags = if awake { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
    // SAFETY: plain Win32 call without pointers.
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        tracing::debug!("SetThreadExecutionState failed");
    }
}

#[cfg(not(windows))]
fn apply(_awake: bool) {}
