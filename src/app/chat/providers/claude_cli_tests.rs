use super::*;

fn cfg(model: &str, reasoning: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        model: model.into(),
        base_url: None,
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: reasoning.map(str::to_string),
        verbosity: None,
        pro_mode: false,
    }
}

#[test]
fn the_run_has_no_tools_and_none_of_the_users_own_setup() {
    let a = args(&cfg("", None), Path::new("/tmp/x"));
    assert_eq!(a.first().map(String::as_str), Some("-p"));
    let after = |flag: &str| a[a.iter().position(|s| s == flag).unwrap() + 1].as_str();
    assert_eq!(after("--tools"), "", "no built-in tool at all");
    assert_eq!(after("--setting-sources"), "", "no hooks or plugins");
    assert!(a.iter().any(|s| s == "--strict-mcp-config"));
    assert!(a.iter().any(|s| s == "--no-session-persistence"));
    assert!(
        !a.iter().any(|s| s == "--bare"),
        "it would drop the sign-in"
    );
    assert!(!a.iter().any(|s| s == "--model" || s == "--effort"));
    assert!(
        !a.iter()
            .any(|s| s.contains("dangerously") || s.contains("bypass")),
        "never a bypass option"
    );
}

#[test]
fn model_and_effort_of_the_profile_are_passed_on() {
    let a = args(&cfg("sonnet", Some(" high ")), Path::new("/tmp/x"));
    let after = |flag: &str| a[a.iter().position(|s| s == flag).unwrap() + 1].as_str();
    assert_eq!(after("--model"), "sonnet");
    assert_eq!(after("--effort"), "high");
}

// Lines as Claude Code 2.1.322 printed them on 2026-11-11, shortened.
const DELTA: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ong"}},"session_id":"s","parent_tool_use_id":null}"#;
const START: &str = r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"s"}"#;
const WHOLE: &str = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"pong"}]}}"#;
const RESULT: &str = r#"{"duration_api_ms":952,"stop_reason":"end_turn","type":"result","is_error":false,"result":"pong","usage":{"input_tokens":2,"cache_creation_input_tokens":641,"cache_read_input_tokens":10,"output_tokens":4}}"#;
const FAILED: &str = r#"{"type":"result","subtype":"error","is_error":true,"result":"Not logged in. Please run /login"}"#;

#[test]
fn only_text_deltas_become_text() {
    assert!(matches!(handle_line(DELTA), Some(ChatEvent::TextDelta(t)) if t == "ong"));
    assert!(handle_line(START).is_none());
    // The whole message repeats what the deltas already carried.
    assert!(handle_line(WHOLE).is_none());
    assert!(handle_line("not json").is_none());
    assert!(handle_line(r#"{"type":"rate_limit_event"}"#).is_none());
}

#[test]
fn the_last_line_carries_the_token_counts_cached_ones_included() {
    assert!(matches!(
        handle_line(RESULT),
        Some(ChatEvent::Usage {
            input_tokens: 653,
            output_tokens: 4
        })
    ));
}

#[test]
fn a_failed_run_says_why() {
    assert!(
        matches!(handle_line(FAILED), Some(ChatEvent::Error(e)) if e.contains("Not logged in"))
    );
}

/// Talks to the real service on the signed-in account, so it only runs when
/// asked for: `OCTA_TEST_CLAUDE_CLI=1 cargo test --bin octa claude_cli -- --ignored`.
#[test]
#[ignore = "needs a signed-in Claude Code CLI and spends plan allowance"]
fn the_real_program_answers_and_asks_for_a_tool() {
    if std::env::var("OCTA_TEST_CLAUDE_CLI").is_err() {
        return;
    }
    let ask = |question: &str, tools: &[ToolDef]| {
        let mut events = Vec::new();
        ClaudeCli
            .stream_turn(
                &cfg("haiku", None),
                "You are a data assistant. Be brief.",
                &[Message::user_text(question)],
                tools,
                &AtomicBool::new(false),
                &mut |e| events.push(e),
            )
            .unwrap();
        events
    };

    let events = ask("Reply with the single word pong.", &[]);
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::TextDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert!(text.to_lowercase().contains("pong"), "{text}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::Usage { output_tokens, .. } if *output_tokens > 0)),
        "token counts reach the meter"
    );

    let count_rows = ToolDef {
        name: "count_rows".into(),
        description: "Count the rows of an open tab.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {"open_tab": {"type": "string", "description": "\"@active\" or a tab name"}}
        }),
    };
    let events = ask("How many rows does the active tab have?", &[count_rows]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::ToolCall { name, .. } if name == "count_rows")),
        "{events:?}"
    );
}

/// The Ask box of the SQL panel on a subscription: question in, one SELECT
/// out. `OCTA_TEST_CLAUDE_CLI=1 cargo test --bin octa claude_cli -- --ignored`.
#[test]
#[ignore = "needs a signed-in Claude Code CLI and spends plan allowance"]
fn the_ask_box_of_the_sql_panel_gets_a_select_from_the_real_program() {
    use crate::app::chat::ask_sql::{self, AskSqlFacts};
    if std::env::var("OCTA_TEST_CLAUDE_CLI").is_err() {
        return;
    }
    let column = |name: &str, data_type: &str| octa::data::ColumnInfo {
        name: name.to_string(),
        data_type: data_type.to_string(),
    };
    let facts = AskSqlFacts {
        table_name: "data".to_string(),
        dialect: "DuckDB".to_string(),
        columns: vec![column("country", "Utf8"), column("amount", "Float64")],
        row_count: Some(42),
        related: String::new(),
        workspace: String::new(),
    };
    let sql = ask_sql::ask(
        &ClaudeCli,
        &cfg("haiku", None),
        &ask_sql::build_prompt(&facts, "total amount per country"),
        "total amount per country",
        &AtomicBool::new(false),
        &mut (0, 0),
    )
    .unwrap();
    let lower = sql.to_lowercase();
    assert!(
        lower.starts_with("select") || lower.starts_with("with"),
        "{sql}"
    );
    assert!(
        lower.contains("group by") && lower.contains("country"),
        "{sql}"
    );
}
