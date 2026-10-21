//! User settings. Persisted as one JSON document; unknown or missing fields fall back to defaults so
//! older and newer versions can read each other's settings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MAX_CONNECTIONS: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OverwritePolicy {
    /// The UI asks every time.
    #[default]
    Ask,
    /// Pick a free name such as `file (1).zip`.
    Rename,
    /// Replace the existing file.
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    /// Use the Windows / environment proxy configuration.
    #[default]
    System,
    None,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    /// `http://host:port`, `https://host:port` or `socks5://host:port` when `mode` is `manual`.
    pub url: String,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self { mode: ProxyMode::System, url: String::new() }
    }
}

/// A daily time window, `HH:MM` in local time. `end` earlier than `start` wraps past midnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TimeWindow {
    pub enabled: bool,
    pub start: String,
    pub end: String,
    /// Days the window applies to, 0 = Sunday … 6 = Saturday. Empty means every day.
    pub days: Vec<u8>,
}

impl Default for TimeWindow {
    fn default() -> Self {
        Self { enabled: false, start: "01:00".into(), end: "07:00".into(), days: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduledLimit {
    pub window: TimeWindow,
    /// Bytes per second while inside the window. 0 = unlimited.
    pub limit: u64,
}

impl Default for ScheduledLimit {
    fn default() -> Self {
        Self {
            window: TimeWindow { enabled: false, start: "09:00".into(), end: "18:00".into(), days: vec![1, 2, 3, 4, 5] },
            limit: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub notify_on_complete: bool,
    pub notify_on_failure: bool,
    pub language: String,

    // Downloads
    pub default_dir: String,
    pub auto_categorize: bool,
    /// Category key → folder. Overrides the automatic folder for that category.
    pub category_folders: BTreeMap<String, String>,
    pub overwrite_policy: OverwritePolicy,
    pub max_active: u32,
    pub default_connections: u8,
    /// Attempts per connection without progress before a download fails.
    pub max_retries: u32,
    /// Upper bound for the exponential retry delay.
    pub retry_max_delay_secs: u64,
    pub resume_on_startup: bool,

    // Network
    pub proxy: ProxySettings,
    /// Global limit in bytes/s. 0 = unlimited.
    pub global_speed_limit: u64,
    pub scheduled_limit: ScheduledLimit,
    pub connect_timeout_secs: u64,
    pub read_timeout_secs: u64,

    // Scheduling
    /// When enabled, the queue only starts downloads inside this window and pauses them when it ends.
    pub queue_window: TimeWindow,

    // Appearance (used by the UI only)
    pub theme: String,
    pub accent: String,
    pub reduced_motion: String,
    pub density: String,

    // Privacy & integration
    pub clipboard_monitor: bool,
    pub diagnostic_logs: bool,
    /// Browser extension origins the user approved.
    pub approved_origins: Vec<String>,
    pub show_add_dialog_for_browser: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            start_minimized: false,
            minimize_to_tray: false,
            close_to_tray: true,
            notify_on_complete: true,
            notify_on_failure: true,
            language: "en".into(),
            default_dir: String::new(),
            auto_categorize: false,
            category_folders: BTreeMap::new(),
            overwrite_policy: OverwritePolicy::Ask,
            max_active: 3,
            default_connections: 8,
            max_retries: 5,
            retry_max_delay_secs: 60,
            resume_on_startup: true,
            proxy: ProxySettings::default(),
            global_speed_limit: 0,
            scheduled_limit: ScheduledLimit::default(),
            connect_timeout_secs: 20,
            read_timeout_secs: 60,
            queue_window: TimeWindow::default(),
            theme: "system".into(),
            accent: "cyan".into(),
            reduced_motion: "system".into(),
            density: "comfortable".into(),
            clipboard_monitor: false,
            diagnostic_logs: true,
            approved_origins: Vec::new(),
            show_add_dialog_for_browser: true,
        }
    }
}

impl Settings {
    /// Clamp values into supported ranges so a hand-edited database cannot break the engine.
    pub fn normalized(mut self) -> Self {
        self.max_active = self.max_active.clamp(1, 20);
        self.default_connections = self.default_connections.clamp(1, MAX_CONNECTIONS);
        self.max_retries = self.max_retries.min(50);
        self.retry_max_delay_secs = self.retry_max_delay_secs.clamp(1, 3600);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(3, 300);
        self.read_timeout_secs = self.read_timeout_secs.clamp(5, 600);
        self
    }
}

/// Parse `HH:MM` into minutes after midnight.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

impl TimeWindow {
    /// Whether `minute` (after local midnight) on `weekday` (0 = Sunday) falls inside the window.
    pub fn contains(&self, weekday: u8, minute: u32) -> bool {
        if !self.enabled {
            return false;
        }
        let (Some(start), Some(end)) = (parse_hhmm(&self.start), parse_hhmm(&self.end)) else {
            return false;
        };
        let day_ok = |d: u8| self.days.is_empty() || self.days.contains(&d);
        if start == end {
            return day_ok(weekday);
        }
        if start < end {
            day_ok(weekday) && minute >= start && minute < end
        } else if minute >= start {
            // Window wraps past midnight: the evening part belongs to today…
            day_ok(weekday)
        } else if minute < end {
            // …and the early-morning part belongs to the day the window started.
            day_ok((weekday + 6) % 7)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_simple_and_wrapping() {
        let w = TimeWindow { enabled: true, start: "09:00".into(), end: "17:00".into(), days: vec![] };
        assert!(w.contains(1, 9 * 60));
        assert!(!w.contains(1, 17 * 60));
        let night = TimeWindow { enabled: true, start: "23:00".into(), end: "06:00".into(), days: vec![5] };
        assert!(night.contains(5, 23 * 60 + 30));
        // Saturday 02:00 belongs to Friday's window.
        assert!(night.contains(6, 120));
        assert!(!night.contains(5, 120));
    }

    #[test]
    fn hhmm() {
        assert_eq!(parse_hhmm("07:30"), Some(450));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("x"), None);
    }
}
