//! Saved SQL snippets: a small named query library persisted to
//! `<config_dir>/sql_snippets.json`. Each snippet carries a name, a free-text
//! description, and the SQL itself. The SQL panel offers them in a **Snippets**
//! dropdown (insert into the editor) and a "Save current query..." flow.

use std::path::PathBuf;

use octa::ui::settings::AppSettings;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SqlSnippet {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) query: String,
}

fn snippets_path() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("sql_snippets.json"))
}

/// Load saved snippets. Missing / unreadable file -> empty list.
pub(crate) fn load() -> Vec<SqlSnippet> {
    let Some(path) = snippets_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Persist the snippet list (best-effort; errors ignored, like settings saves).
pub(crate) fn save(snippets: &[SqlSnippet]) {
    let Some(path) = snippets_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(snippets) {
        let _ = std::fs::write(&path, text);
    }
}
