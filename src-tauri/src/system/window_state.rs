//! Remember the main window's size, position and maximised state between launches.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

const FILE: &str = "window-state.json";

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Saved {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

fn read(dir: &Path) -> Option<Saved> {
    serde_json::from_slice(&std::fs::read(dir.join(FILE)).ok()?).ok()
}

/// Save the current geometry. Minimised windows are skipped, and while maximised only the flag is
/// updated so the normal size is restored after un-maximising next time.
pub fn save(window: &WebviewWindow, dir: &Path) {
    if window.is_minimized().unwrap_or(false) || !window.is_visible().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let next = match (maximized, read(dir)) {
        (true, Some(prev)) => Saved { maximized: true, ..prev },
        _ => {
            let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else { return };
            Saved { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized }
        }
    };
    if let Ok(json) = serde_json::to_vec(&next) {
        let _ = std::fs::write(dir.join(FILE), json);
    }
}

/// First launch: make sure the default size fits the screen (small or highly scaled displays would
/// otherwise get a window taller than the work area, which Windows shows maximised) and centre it.
fn fit_first_launch(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else { return };
    let area = monitor.work_area().size;
    let Ok(size) = window.inner_size() else { return };
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    let max_w = (area.width as f64 * 0.92) as u32;
    let max_h = (area.height as f64 * 0.92) as u32;
    if size.width > max_w || size.height > max_h {
        let _ = window.set_size(PhysicalSize::new(size.width.min(max_w), size.height.min(max_h)));
    }
    let _ = window.center();
}

/// Restore geometry if it still fits on a connected monitor (screens may have changed since).
pub fn restore(window: &WebviewWindow, dir: &Path) {
    let Some(s) = read(dir) else {
        fit_first_launch(window);
        return;
    };
    let monitors = window.available_monitors().unwrap_or_default();
    let visible = monitors.iter().any(|m| {
        let (p, z) = (m.position(), m.size());
        let (cx, cy) = (s.x + 80, s.y + 20);
        cx >= p.x && cy >= p.y && cx < p.x + z.width as i32 && cy < p.y + z.height as i32
    });
    if visible && s.width >= 600 && s.height >= 400 {
        let _ = window.set_size(PhysicalSize::new(s.width, s.height));
        let _ = window.set_position(PhysicalPosition::new(s.x, s.y));
    }
    if s.maximized {
        let _ = window.maximize();
    }
}
