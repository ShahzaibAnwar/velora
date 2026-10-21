//! Give the main window proper large and small icons on Windows.
//!
//! The window library only sets a small (ICON_SMALL) icon, so the taskbar, which uses the large icon,
//! fell back to a generic picture. We load the multi-size icon embedded in velora.exe at exactly the
//! sizes Windows asks for at the window's current DPI, and set both on the window and its class.

/// Resource ID of the application icon that tauri-build embeds in the executable.
#[cfg(windows)]
const APP_ICON_ID: usize = 32512;

#[cfg(windows)]
pub fn apply(hwnd: isize) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadImageW, SendMessageW, SetClassLongPtrW, GCLP_HICON, GCLP_HICONSM, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXICON,
        SM_CXSMICON, WM_SETICON,
    };

    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    // SAFETY: `hwnd` is our live top-level window; the module handle is our own executable, and the
    // icon handles stay valid for the life of the process (they are intentionally never destroyed).
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let dpi = match GetDpiForWindow(hwnd) {
            0 => 96,
            d => d,
        };
        let load = |metric| {
            let size = GetSystemMetricsForDpi(metric, dpi);
            LoadImageW(module, APP_ICON_ID as *const u16, IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
        };
        let big = load(SM_CXICON);
        let small = load(SM_CXSMICON);
        if !big.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big as isize);
            SetClassLongPtrW(hwnd, GCLP_HICON, big as isize);
        }
        if !small.is_null() {
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small as isize);
            SetClassLongPtrW(hwnd, GCLP_HICONSM, small as isize);
        }
    }
}

#[cfg(not(windows))]
pub fn apply(_hwnd: isize) {}

/// Declare the same AppUserModelID that the installer stamps on Velora's shortcuts. Without it the
/// taskbar can't match the window to its shortcut and shows a placeholder icon; notifications also
/// use this ID. Must run before any window is created.
#[cfg(windows)]
pub fn set_app_user_model_id(id: &str) {
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
    let hr = unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
    if hr < 0 {
        tracing::warn!(hr, "couldn't set AppUserModelID");
    }
}

#[cfg(not(windows))]
pub fn set_app_user_model_id(_id: &str) {}
