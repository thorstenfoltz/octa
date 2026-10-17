//! Export a saved chat session to a human-readable Markdown transcript. JSON
//! export reuses `persist::snapshot` + `serde_json` at the call site; this
//! module owns only the Markdown rendering.

use crate::app::chat::persist::SavedSession;
use crate::app::chat::types::{ContentBlock, Role};

/// Per-tool-result truncation cap. Tool results can be large table dumps; the
/// Markdown transcript keeps the first `EXPORT_RESULT_CAP_BYTES` of each and
/// notes the truncation.
// ponytail: a constant, not a setting. Promote only if a user asks.
pub const EXPORT_RESULT_CAP_BYTES: usize = 2048;

/// Truncate `s` to at most `cap` bytes without splitting a UTF-8 char.
fn truncate_on_char_boundary(s: &str, cap: usize) -> &str {
    if s.len() <= cap {
        return s;
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Render a session as a Markdown transcript: prose, every tool call (SQL
/// queries in ```sql blocks), and truncated tool results.
pub fn to_markdown(session: &SavedSession, cap_bytes: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", session.title));
    out.push_str(&format!(
        "_Provider: {} - Model: {}_\n\n",
        session.provider, session.model
    ));

    for msg in &session.messages {
        match msg.role {
            Role::User => out.push_str("## You\n\n"),
            Role::Assistant => out.push_str("## Assistant\n\n"),
            Role::Tool | Role::System => {}
        }
        for block in &msg.blocks {
            match block {
                ContentBlock::Text { text } => {
                    if !text.trim().is_empty() {
                        out.push_str(text.trim_end());
                        out.push_str("\n\n");
                    }
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    if name == "run_sql"
                        && let Some(q) = input.get("query").and_then(|v| v.as_str())
                    {
                        out.push_str(&format!(
                            "**SQL** (`run_sql`):\n\n```sql\n{}\n```\n\n",
                            q.trim()
                        ));
                        continue;
                    }
                    let args = serde_json::to_string_pretty(input).unwrap_or_default();
                    out.push_str(&format!(
                        "**Tool** `{}`:\n\n```json\n{}\n```\n\n",
                        name, args
                    ));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let kind = if *is_error { "Error" } else { "Result" };
                    let truncated = content.len() > cap_bytes;
                    let shown = truncate_on_char_boundary(content, cap_bytes);
                    let note = if truncated {
                        format!(" (truncated to {cap_bytes} bytes)")
                    } else {
                        String::new()
                    };
                    out.push_str(&format!(
                        "_{kind}{note}:_\n\n```\n{}\n```\n\n",
                        shown.trim_end()
                    ));
                }
                // Opaque provider payloads (e.g. encrypted reasoning items)
                // carry nothing readable; skip them in the export.
                ContentBlock::ProviderData { .. } => {}
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
