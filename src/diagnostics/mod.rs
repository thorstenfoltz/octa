//! Diagnostics: file logging, crash capture, and a redacted debug report.

pub mod crash;
pub mod report;
mod rotating_writer;

pub use rotating_writer::RotatingWriter;

use std::path::PathBuf;
use std::sync::OnceLock;

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt, reload};

use crate::ui::settings::AppSettings;

/// Stored once `init_logging` runs; flips the filter live when debug mode toggles.
static RELOAD: OnceLock<Box<dyn Fn(bool) + Send + Sync>> = OnceLock::new();

/// Directory for all diagnostic files (rolling log, crash report, run lock):
/// `config_dir/logs`. Keeps the config directory itself uncluttered.
/// ponytail: old files left directly in config_dir from before this move are
/// not migrated; a fresh log just starts under logs/.
pub fn logs_dir() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("logs"))
}

fn directive(debug: bool) -> &'static str {
    // Our crate at info (or debug); noisy deps only at warn+. Keeps the log
    // readable and smaller.
    if debug {
        "warn,octa=debug"
    } else {
        "warn,octa=info"
    }
}

/// Initialise GUI file logging to `config_dir/logs/octa.log` (size-capped).
/// Best effort: if the directory or file is unavailable, logging is simply off.
/// Call once, early, on the GUI path only (the MCP path keeps its stderr setup).
pub fn init_logging(debug_mode: bool) {
    let Some(dir) = logs_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(writer) = RotatingWriter::open(&dir.join("octa.log")) else {
        return;
    };

    // RUST_LOG still overrides the startup default, matching the MCP path.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directive(debug_mode)));
    let (filter_layer, handle) = reload::Layer::new(filter);

    let fmt_layer = fmt::layer()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_target(true);

    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init();

    // The live toggle forces the explicit directive (ignores RUST_LOG) so it
    // visibly takes effect.
    let _ = RELOAD.set(Box::new(move |on: bool| {
        let _ = handle.reload(EnvFilter::new(directive(on)));
    }));
}

/// Flip log verbosity live when the user toggles debug mode in Settings.
pub fn set_debug_level(on: bool) {
    if let Some(f) = RELOAD.get() {
        f(on);
    }
}
