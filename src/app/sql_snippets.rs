//! Saved SQL snippets: a small named query library persisted to
//! `<config_dir>/sql_snippets.json`. Each snippet carries a name, a free-text
//! description, and the SQL itself. The SQL panel offers them in a **Snippets**
//! dropdown (insert into the editor) and a "Save current query..." flow.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SqlSnippet {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) query: String,
}

const FILE: &str = "sql_snippets.json";

/// Load saved snippets. Missing / unreadable file -> empty list.
pub(crate) fn load() -> Vec<SqlSnippet> {
    super::config_json::load(FILE)
}

/// Persist the snippet list (best-effort; errors ignored, like settings saves).
pub(crate) fn save(snippets: &[SqlSnippet]) {
    super::config_json::save(FILE, snippets)
}
