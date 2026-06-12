//! Saved chat prompts: a small named library of reusable assistant prompts
//! persisted to `<config_dir>/chat_prompts.json`. Each prompt carries a name,
//! a free-text description, and the prompt body itself. The chat panel offers
//! them in a **Prompts** dropdown (insert into the input) and a "Save current
//! prompt..." flow. Mirrors `src/app/sql_snippets.rs`.

use std::path::PathBuf;

use octa::ui::settings::AppSettings;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatPrompt {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) text: String,
}

fn prompts_path() -> Option<PathBuf> {
    AppSettings::config_dir().map(|d| d.join("chat_prompts.json"))
}

/// Load saved prompts. Missing / unreadable file -> empty list.
pub(crate) fn load() -> Vec<ChatPrompt> {
    let Some(path) = prompts_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Persist the prompt list (best-effort; errors ignored, like settings saves).
pub(crate) fn save(prompts: &[ChatPrompt]) {
    let Some(path) = prompts_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(prompts) {
        let _ = std::fs::write(&path, text);
    }
}
