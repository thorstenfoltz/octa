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
