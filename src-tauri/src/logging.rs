//! File logging with a runtime-adjustable level. Diagnostic logging can be switched off in Settings,
//! leaving only errors. URLs are redacted by the engine before they are logged.

use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter, Registry};

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub struct LogControl {
    handle: Option<reload::Handle<EnvFilter, Registry>>,
}

fn filter(verbose: bool) -> EnvFilter {
    EnvFilter::new(if verbose { "info,velora_engine=info,velora_lib=info" } else { "error" })
}

pub fn init(log_dir: &Path, verbose: bool) -> LogControl {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("velora")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir);
    let Ok(appender) = appender else {
        return LogControl { handle: None };
    };
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = GUARD.set(guard);
    let (filter_layer, handle) = reload::Layer::new(filter(verbose));
    let file_layer = fmt::layer().with_writer(writer).with_ansi(false).with_target(true);
    let registry = tracing_subscriber::registry().with(filter_layer).with(file_layer);
    #[cfg(debug_assertions)]
    let result = registry.with(fmt::layer().with_writer(std::io::stderr)).try_init();
    #[cfg(not(debug_assertions))]
    let result = registry.try_init();
    if result.is_err() {
        return LogControl { handle: None };
    }
    LogControl { handle: Some(handle) }
}

impl LogControl {
    pub fn set_verbose(&self, verbose: bool) {
        if let Some(h) = &self.handle {
            let _ = h.modify(|f| *f = filter(verbose));
        }
    }
}
