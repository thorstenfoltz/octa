//! Chat tool-call audit log. When enabled (`chat_audit_log_enabled`), every
//! tool the assistant runs is appended as one JSON line to
//! `<config_dir>/chat_audit/<session_id>.jsonl` (tool name, argument + result
//! byte counts, duration, error flag, timestamp). Off by default.
//!
//! On startup Octa sums the size of all audit files and, if it exceeds
//! `chat_audit_log_warn_bytes` (default 10 MB) and `chat_audit_log_warn_enabled`
//! is on, shows a one-time warning so the logs don't grow unbounded unnoticed.

use std::io::Write;
use std::path::PathBuf;

use octa::ui::settings::AppSettings;
use serde::Serialize;

/// `<config_dir>/chat_audit`.
pub(crate) fn audit_dir() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("chat_audit"))
}

/// One recorded tool call.
#[derive(Serialize)]
pub(crate) struct AuditEntry {
    pub ts_unix: u64,
    pub tool: String,
    pub args_bytes: usize,
    pub result_bytes: usize,
    pub duration_ms: u128,
    pub is_error: bool,
}

/// Append `entry` to the session's audit file (best-effort; errors ignored).
pub(crate) fn record(session_id: &str, entry: &AuditEntry) {
    let Some(dir) = audit_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    // Session ids are generated, but sanitise defensively so a crafted id can
    // never escape the audit directory.
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        return;
    }
    let path = dir.join(format!("{safe}.jsonl"));
    if let Ok(line) = serde_json::to_string(entry)
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Total byte size of every file in the audit directory.
pub(crate) fn total_size_bytes() -> u64 {
    let Some(dir) = audit_dir() else {
        return 0;
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return 0;
    };
    rd.flatten()
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Current unix time in seconds.
pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
