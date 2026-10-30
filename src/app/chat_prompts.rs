//! Saved chat prompts: a small named library of reusable assistant prompts
//! persisted to `<config_dir>/chat_prompts.json`. Each prompt carries a name,
//! a free-text description, and the prompt body itself. The chat panel offers
//! them in a **Prompts** dropdown (insert into the input) and a "Save current
//! prompt..." flow. Mirrors `src/app/sql_snippets.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatPrompt {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) text: String,
}

const FILE: &str = "chat_prompts.json";

/// Load saved prompts. Missing / unreadable file -> empty list.
pub(crate) fn load() -> Vec<ChatPrompt> {
    super::config_json::load(FILE)
}

/// Persist the prompt list (best-effort; errors ignored, like settings saves).
pub(crate) fn save(prompts: &[ChatPrompt]) {
    super::config_json::save(FILE, prompts)
}
