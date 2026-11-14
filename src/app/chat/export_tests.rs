use super::*;
use crate::app::chat::session::TraceToolRun;
use crate::app::chat::types::Message;
use serde_json::json;

fn one_step() -> TraceStep {
    TraceStep {
        model: "claude".into(),
        system: "You are the system prompt".into(),
        tools: vec!["run_sql".into(), "schema".into()],
        sent: "{\"question\":\"top customers\"}".into(),
        reply: "Here they are, with a fence: ```x```".into(),
        input_tokens: 1200,
        output_tokens: 34,
        millis: Some(2500),
        tool_runs: vec![TraceToolRun {
            name: "run_sql".into(),
            input: "{\"query\":\"SELECT 1\"}".into(),
            output: "a,b\n1,2".into(),
            is_error: true,
            millis: 40,
        }],
        error: Some("the server went away".into()),
    }
}

/// Off by default means off to the byte: the setting must not change a file
/// anyone already relies on.
#[test]
fn with_no_trace_the_export_is_what_it_always_was() {
    let session = sample();
    assert_eq!(
        to_json(&session, &[]),
        serde_json::to_string_pretty(&session).unwrap()
    );
    assert_eq!(trace_to_markdown(&[]), "");
}

#[test]
fn the_trace_adds_everything_the_debug_box_shows() {
    let md = trace_to_markdown(&[one_step()]);
    for part in [
        "## What happened",
        "### Step 1",
        "1200 tokens in",
        "34 out",
        "2.5 s",
        "claude",
        "You are the system prompt",
        "run_sql, schema",
        "top customers",
        "Here they are",
        "SELECT 1",
        "a,b\n1,2",
        "the server went away",
    ] {
        assert!(md.contains(part), "{part:?} missing from:\n{md}");
    }
}

/// A reply can hold a code block of its own, which must not end the one around it.
#[test]
fn text_with_a_code_fence_stays_inside_its_block() {
    let md = trace_to_markdown(&[one_step()]);
    assert!(
        md.contains("````\nHere they are, with a fence: ```x```\n````"),
        "{md}"
    );
}

#[test]
fn the_json_export_carries_the_trace_beside_the_session() {
    let session = sample();
    let value: serde_json::Value = serde_json::from_str(&to_json(&session, &[one_step()])).unwrap();
    assert_eq!(value["title"], "Test chat");
    assert_eq!(value["trace"][0]["system"], "You are the system prompt");
    assert_eq!(value["trace"][0]["tool_runs"][0]["name"], "run_sql");
}

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
