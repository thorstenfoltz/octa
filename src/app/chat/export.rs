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
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chat::types::Message;
    use serde_json::json;

    fn sample() -> SavedSession {
        SavedSession {
            id: "abc".into(),
            title: "Test chat".into(),
            provider: "anthropic".into(),
            model: "claude".into(),
            created_unix: 0,
            updated_unix: 0,
            messages: vec![
                Message::user_text("show top customers"),
                Message::assistant(vec![
                    ContentBlock::Text {
                        text: "Running a query.".into(),
                    },
                    ContentBlock::ToolUse {
                        id: "t1".into(),
                        name: "run_sql".into(),
                        input: json!({ "query": "SELECT * FROM data LIMIT 1" }),
                    },
                ]),
                Message::tool_results(vec![ContentBlock::ToolResult {
                    id: "t1".into(),
                    content: "a,b\n1,2".into(),
                    is_error: false,
                }]),
            ],
        }
    }

    #[test]
    fn markdown_includes_sql_block_and_prose() {
        let md = to_markdown(&sample(), EXPORT_RESULT_CAP_BYTES);
        assert!(md.contains("## You"));
        assert!(md.contains("show top customers"));
        assert!(md.contains("```sql\nSELECT * FROM data LIMIT 1\n```"));
        assert!(md.contains("Running a query."));
        assert!(md.contains("_Result:_"));
    }

    #[test]
    fn long_results_are_truncated() {
        let mut s = sample();
        if let Some(Message { blocks, .. }) = s.messages.last_mut()
            && let Some(ContentBlock::ToolResult { content, .. }) = blocks.last_mut()
        {
            *content = "x".repeat(5000);
        }
        let md = to_markdown(&s, 100);
        assert!(md.contains("(truncated to 100 bytes)"));
    }
}
