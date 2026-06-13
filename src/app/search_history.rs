//! Persistent search history. The last N distinct search queries are kept in
//! `<config_dir>/search_history.json` (most-recent first) and offered back in
//! a dropdown next to the search box. N is the `search_history_limit` setting
//! (0 disables it).

use std::path::PathBuf;

use octa::ui::settings::AppSettings;

fn history_path() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("search_history.json"))
}

/// Load the saved history (most-recent first). Missing / unreadable file -> empty.
pub(crate) fn load() -> Vec<String> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Persist the history (best-effort; errors are ignored, like settings saves).
fn save(history: &[String]) {
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

/// Record `query` as the most-recent entry: trim, skip empties, de-duplicate
/// (case-sensitively), cap to `limit`, and persist. `limit == 0` clears and
/// disables the history.
pub(crate) fn record(history: &mut Vec<String>, query: &str, limit: usize) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    if limit == 0 {
        if !history.is_empty() {
            history.clear();
            save(history);
        }
        return;
    }
    history.retain(|q| q != query);
    history.insert(0, query.to_string());
    history.truncate(limit);
    save(history);
}
