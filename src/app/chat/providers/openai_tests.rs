//! Unit tests for [`openai`](openai). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

#[test]
fn flattens_tool_use_and_results() {
    let messages = vec![
        Message::user_text("how many rows?"),
        Message::assistant(vec![ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "count_rows".into(),
            input: json!({"open_tab": "@active"}),
        }]),
        Message::tool_results(vec![ContentBlock::ToolResult {
            id: "call_1".into(),
            content: "{\"row_count\":42}".into(),
            is_error: false,
        }]),
    ];
    let wire = messages_to_wire("be helpful", &messages);
    // system, user, assistant(with tool_calls), tool.
    assert_eq!(wire[0]["role"], "system");
    assert_eq!(wire[1]["role"], "user");
    assert_eq!(wire[2]["role"], "assistant");
    assert_eq!(wire[2]["content"], Value::Null);
    assert_eq!(wire[2]["tool_calls"][0]["id"], "call_1");
    assert_eq!(wire[2]["tool_calls"][0]["function"]["name"], "count_rows");
    assert_eq!(wire[3]["role"], "tool");
    assert_eq!(wire[3]["tool_call_id"], "call_1");
}

#[test]
fn tool_schema_wraps_under_function() {
    let cfg = ProviderConfig {
        model: "gpt-4o".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: 0.5,
        max_tokens: Some(100),
    };
    let tools = vec![ToolDef {
        name: "schema".into(),
        description: "desc".into(),
        input_schema: json!({"type": "object", "properties": {}}),
    }];
    let body = build_body(
        &cfg,
        "sys",
        &[Message::user_text("hi")],
        &tools,
        "max_completion_tokens",
    );
    assert_eq!(body["stream"], true);
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "schema");
    assert_eq!(body["tool_choice"], "auto");
    // The chosen token field carries the cap; the legacy field is absent.
    assert_eq!(body["max_completion_tokens"], 100);
    assert!(body.get("max_tokens").is_none());
}

#[test]
fn unlimited_tokens_omits_the_field() {
    let cfg = ProviderConfig {
        model: "gpt-4o".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: 0.5,
        max_tokens: None,
    };
    let body = build_body(&cfg, "sys", &[Message::user_text("hi")], &[], "max_tokens");
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("max_completion_tokens").is_none());
}
