//! What the Settings dialog knows about the assistant's tools.
//!
//! The tools themselves live in the binary (`app::chat`), because they are the
//! MCP tools and their `schemars` schemas; this dialog lives in the library.
//! So the binary publishes the list once at startup and the settings list
//! reads it here. Before that call, and in a library-only test, the list is
//! empty and the section simply does not render.
//!
//! Names, group ids and the English one-liners are deliberately *not*
//! translated: they are the same words the model is given in the `enable_tools`
//! menu, and a settings list that said something different from what the model
//! was told would be worse than a settings list in English.

use std::sync::OnceLock;

/// One tool, as the settings list needs it.
pub struct ToolInfo {
    /// Wire name, e.g. `run_sql`.
    pub name: &'static str,
    /// Group id, e.g. `quality`. Tools arrive grouped, in display order.
    pub group: &'static str,
    /// One line on what the group is for.
    pub group_summary: &'static str,
    /// True for the group that goes out with every request.
    pub always_sent: bool,
    /// What the tool does (the description the model gets).
    pub description: String,
    /// When a person would want it switched on.
    pub when: &'static str,
    /// Approximate tokens this tool's definition costs per request.
    pub tokens: usize,
}

static REGISTRY: OnceLock<Vec<ToolInfo>> = OnceLock::new();

/// Publish the tool list. Called once by the binary at startup; later calls
/// are ignored, so a second call cannot swap the list out from under the UI.
pub fn publish(tools: Vec<ToolInfo>) {
    let _ = REGISTRY.set(tools);
}

/// The published tools, in group order. Empty until the binary publishes.
pub fn all() -> &'static [ToolInfo] {
    REGISTRY.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Tokens per request for the tools that are on, split into the ones sent with
/// every request and the ones a group fetch would add.
pub fn token_split(disabled: &[String]) -> (usize, usize) {
    let mut always = 0;
    let mut on_demand = 0;
    for tool in all() {
        if disabled.iter().any(|d| d == tool.name) {
            continue;
        }
        if tool.always_sent {
            always += tool.tokens;
        } else {
            on_demand += tool.tokens;
        }
    }
    (always, on_demand)
}
