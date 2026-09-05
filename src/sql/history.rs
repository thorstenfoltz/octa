//! Query history: every SQL statement actually run, with what it cost.
//!
//! Snippets are for a query you decided to keep. History is for the one you ran
//! twenty minutes ago and did not, which until now was thrown away when the tab
//! closed. Persisted to `<config_dir>/sql_history.json` and **scoped**, so the
//! queries you ran against the production server are not mixed in with the ones
//! you ran against a CSV: a database tab keys on the connection id, a
//! file-backed workspace on its path.
//!
//! Each entry carries the timing and row count the panel already had in hand,
//! which is what makes the list worth reading rather than just re-runnable.
//!
//! Recording is a user setting (`sql_history_enabled`, on by default, capped at
//! `sql_history_limit` = 20). Turning it off stops recording *and* deletes what
//! was kept: a history switch that leaves the old file behind is a nasty
//! surprise for something that can hold literals out of your data.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::ui::settings::AppSettings;
use serde::{Deserialize, Serialize};

/// One executed query and what it cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SqlHistoryEntry {
    pub query: String,
    /// Unix seconds. Plain number rather than a date type: it is only ever
    /// formatted for display, and this keeps the file readable by eye.
    #[serde(default)]
    pub at_unix: u64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub rows: usize,
}

/// Which workspace a query was run in. A connection id survives a rename, so
/// renaming a connection keeps its history; a file path is the best available
/// identity for a local workspace.
pub fn db_scope(conn_id: &str) -> String {
    format!("db:{conn_id}")
}

pub fn file_scope(path: &str) -> String {
    format!("file:{path}")
}

/// The whole history file: scope -> entries, most recent first.
type History = BTreeMap<String, Vec<SqlHistoryEntry>>;

fn history_path() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("sql_history.json"))
}

fn load_all() -> History {
    let Some(path) = history_path() else {
        return History::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return History::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_all(history: &History) {
    let Some(path) = history_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(history) {
        let _ = std::fs::write(&path, text);
    }
}

/// The recorded queries for one scope, most recent first.
pub fn load(scope: &str) -> Vec<SqlHistoryEntry> {
    load_all().remove(scope).unwrap_or_default()
}

/// Fold one executed query into `entries`, newest first, de-duplicated and
/// capped. Pure, so the ordering and capping rules are testable without
/// touching the disk.
///
/// `limit` of 0 means unlimited, the same convention `chat_result_row_limit`
/// uses. Re-running a query moves it to the top with its new timing rather than
/// leaving a second copy: the list is "what have I run", not "how often".
pub fn fold(entries: &mut Vec<SqlHistoryEntry>, entry: SqlHistoryEntry, limit: usize) -> bool {
    let trimmed = entry.query.trim();
    if trimmed.is_empty() {
        return false;
    }
    let entry = SqlHistoryEntry {
        query: trimmed.to_string(),
        ..entry
    };
    entries.retain(|e| e.query != entry.query);
    entries.insert(0, entry);
    if limit > 0 {
        entries.truncate(limit);
    }
    true
}

/// Record one executed query against a scope. A no-op when history is switched
/// off, so the caller need not check first.
///
/// Takes the two settings values rather than `AppSettings` so it can be called
/// while a tab is mutably borrowed, which is the whole of the SQL panel.
pub fn record(scope: &str, entry: SqlHistoryEntry, enabled: bool, limit: usize) {
    if !enabled {
        return;
    }
    let mut all = load_all();
    let entries = all.entry(scope.to_string()).or_default();
    if fold(entries, entry, limit) {
        save_all(&all);
    }
}

/// Forget one scope's history (the panel's Clear history action).
pub fn clear(scope: &str) {
    let mut all = load_all();
    if all.remove(scope).is_some() {
        save_all(&all);
    }
}

/// Delete the whole history file. Called when the user switches recording off,
/// so the setting means "do not keep my queries" rather than only "stop adding
/// to the pile".
pub fn forget_everything() {
    if let Some(path) = history_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Seconds since the unix epoch, for stamping an entry.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
