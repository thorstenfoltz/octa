//! Query history: every SQL statement actually run, with what it cost.
//!
//! Snippets are for a query you decided to keep. History is for the one you ran
//! twenty minutes ago and did not, which until now was thrown away when the tab
//! closed. Persisted to `<config_dir>/sql_history.json` as **one list** shared
//! by every editor, newest first. Each entry remembers where it ran (`source`:
//! a connection id, a cloud object or a file path), so the menu can say so.
//! Earlier builds kept one list per source; those files are flattened on load.
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
    /// Where the query ran: `db:<conn_id>`, `cloud:<conn_id>:<key>`,
    /// `file:<path>` or `scratch`. Ids rather than names, so a renamed
    /// connection still resolves when the menu draws it.
    #[serde(default)]
    pub source: String,
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

pub fn cloud_scope(conn_id: &str, key: &str) -> String {
    format!("cloud:{conn_id}:{key}")
}

/// Parse the history file. Takes the current flat list, or the older
/// `scope -> entries` map, which is flattened (the scope becomes each entry's
/// `source`) and re-sorted newest first so nobody loses their history.
pub fn parse(text: &str) -> Vec<SqlHistoryEntry> {
    if let Ok(v) = serde_json::from_str::<Vec<SqlHistoryEntry>>(text) {
        return v;
    }
    let Ok(old) = serde_json::from_str::<BTreeMap<String, Vec<SqlHistoryEntry>>>(text) else {
        return Vec::new();
    };
    let mut out: Vec<SqlHistoryEntry> = old
        .into_iter()
        .flat_map(|(scope, entries)| {
            entries.into_iter().map(move |e| SqlHistoryEntry {
                source: scope.clone(),
                ..e
            })
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.at_unix));
    out
}

fn history_path() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("sql_history.json"))
}

/// Every recorded query, most recent first.
pub fn load() -> Vec<SqlHistoryEntry> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(&path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

fn save_all(history: &[SqlHistoryEntry]) {
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

/// Fold one executed query into `entries`, newest first, de-duplicated and
/// capped. Pure, so the ordering and capping rules are testable without
/// touching the disk.
///
/// `limit` of 0 means unlimited, the same convention `chat_result_row_limit`
/// uses. Re-running a query moves it to the top with its new timing rather than
/// leaving a second copy: the list is "what have I run", not "how often". The
/// same text run against two sources is two entries.
pub fn fold(entries: &mut Vec<SqlHistoryEntry>, entry: SqlHistoryEntry, limit: usize) -> bool {
    let trimmed = entry.query.trim();
    if trimmed.is_empty() {
        return false;
    }
    let entry = SqlHistoryEntry {
        query: trimmed.to_string(),
        ..entry
    };
    entries.retain(|e| e.query != entry.query || e.source != entry.source);
    entries.insert(0, entry);
    if limit > 0 {
        entries.truncate(limit);
    }
    true
}

/// Record one executed query. A no-op when history is switched off, so the
/// caller need not check first. Re-reads the file rather than trusting the
/// in-memory copy, so a second Octa window's queries are not overwritten.
///
/// Takes the two settings values rather than `AppSettings` so it can be called
/// while a tab is mutably borrowed, which is the whole of the SQL panel.
pub fn record(entry: SqlHistoryEntry, enabled: bool, limit: usize) {
    if !enabled {
        return;
    }
    let mut all = load();
    if fold(&mut all, entry, limit) {
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
