//! Unit tests for [`ollama`](ollama). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

fn cfg() -> ProviderConfig {
    ProviderConfig {
        model: "qwen3.8".into(),
        base_url: Some("http://localhost:11434".into()),
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    }
}

fn tool_round() -> Vec<Message> {
    vec![
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
    ]
}

/// The whole reason this adapter left `/v1/chat/completions`: without an
/// explicit window Ollama sizes one from VRAM (4k on a modest machine), then
/// drops the oldest non-system message to fit - the user's question - and a
/// Qwen 3.8 class model answers `500 no user query found in messages`.
#[test]
fn every_request_asks_for_a_context_window() {
    let body = build_body(&cfg(), "be helpful", &tool_round(), &[]).unwrap();
    assert_eq!(body["options"]["num_ctx"], json!(NUM_CTX));
}

#[test]
fn tool_round_keeps_the_user_question_and_names_the_tool() {
    let wire = messages_to_wire("be helpful", &tool_round());
    // system, user, assistant(with tool_calls), tool.
    assert_eq!(wire[0]["role"], "system");
    assert_eq!(wire[1]["role"], "user");
    assert_eq!(wire[1]["content"], "how many rows?");
    assert_eq!(wire[2]["role"], "assistant");
    // Plain string, never null: the native endpoint has no nullable content.
    assert_eq!(wire[2]["content"], "");
    assert_eq!(wire[2]["tool_calls"][0]["id"], "call_1");
    assert_eq!(wire[2]["tool_calls"][0]["function"]["name"], "count_rows");
    // Arguments are an object here, not the JSON string OpenAI wants.
    assert_eq!(
        wire[2]["tool_calls"][0]["function"]["arguments"],
        json!({"open_tab": "@active"})
    );
    assert_eq!(wire[3]["role"], "tool");
    assert_eq!(wire[3]["tool_call_id"], "call_1");
    assert_eq!(wire[3]["tool_name"], "count_rows");
    assert_eq!(wire.len(), 4);
}

#[test]
fn tools_and_options_land_where_ollama_looks_for_them() {
    let mut c = cfg();
    c.temperature = Some(0.25);
    c.max_tokens = Some(4096);
    c.reasoning = Some("high".into());
    let tools = vec![ToolDef {
        name: "schema".into(),
        description: "columns and types".into(),
        input_schema: json!({"type": "object", "properties": {}}),
    }];
    let body = build_body(&c, "sys", &[Message::user_text("hi")], &tools).unwrap();
    assert_eq!(body["options"]["temperature"], json!(0.25));
    assert_eq!(body["options"]["num_predict"], json!(4096));
    assert_eq!(body["think"], "high");
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["tools"][0]["function"]["name"], "schema");
}

/// The native endpoint's `think` takes a level or a bool, never a budget.
#[test]
fn a_token_budget_is_refused_locally() {
    let mut c = cfg();
    c.reasoning = Some("2048".into());
    let err = build_body(&c, "sys", &[Message::user_text("hi")], &[]).unwrap_err();
    assert!(err.contains("thinking level"), "{err}");
}

/// Every line at the same instant, so no live rate is emitted and the
/// tests below see only the wire events.
fn drain(lines: &[&str]) -> Vec<ChatEvent> {
    let now = Instant::now();
    let mut st = StreamState::default();
    let mut out = Vec::new();
    for line in lines {
        if handle_line(line, now, &mut st, &mut |ev| out.push(ev)) {
            break;
        }
    }
    out
}

/// On a CPU a turn runs for minutes, so the rate is computed live from the
/// tokens received: one line per token, the first one marking t0. Thinking
/// lines count too; the final line's own timing then takes over.
#[test]
fn rate_is_counted_live_and_then_replaced_by_ollamas_figure() {
    let t0 = Instant::now();
    let mut st = StreamState::default();
    let mut rates = Vec::new();
    let mut sink = |ev: ChatEvent| {
        if let ChatEvent::Throughput { tokens_per_second } = ev {
            rates.push(tokens_per_second);
        }
    };
    let lines = [
        (
            0.0,
            r#"{"message":{"role":"assistant","content":"","thinking":"hm"},"done":false}"#,
        ),
        (
            0.5,
            r#"{"message":{"role":"assistant","content":"42"},"done":false}"#,
        ),
        (
            1.0,
            r#"{"message":{"role":"assistant","content":" rows"},"done":false}"#,
        ),
        // Empty content and no thinking: a keep-alive, not a token.
        (
            1.5,
            r#"{"message":{"role":"assistant","content":""},"done":false}"#,
        ),
        (
            2.0,
            r#"{"message":{"role":"assistant","content":""},"done":true,"eval_count":3,"eval_duration":1500000000}"#,
        ),
    ];
    for (secs, line) in lines {
        let now = t0 + std::time::Duration::from_secs_f32(secs);
        handle_line(line, now, &mut st, &mut sink);
    }
    // 1 interval in 0.5s, 2 intervals in 1.0s, then Ollama's 3 tokens / 1.5s.
    assert_eq!(rates.len(), 3, "{rates:?}");
    assert!((rates[0] - 2.0).abs() < 0.01, "{rates:?}");
    assert!((rates[1] - 2.0).abs() < 0.01, "{rates:?}");
    assert!((rates[2] - 2.0).abs() < 0.01, "{rates:?}");
}

#[test]
fn ndjson_stream_becomes_text_then_done() {
    let events = drain(&[
        r#"{"message":{"role":"assistant","content":"42"},"done":false}"#,
        r#"{"message":{"role":"assistant","content":" rows"},"done":false}"#,
        r#"{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":120,"prompt_eval_duration":6000000000,"eval_count":7,"eval_duration":250000000}"#,
    ]);
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::TextDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "42 rows");
    assert!(matches!(
        events[events.len() - 4],
        ChatEvent::Usage {
            input_tokens: 120,
            output_tokens: 7
        }
    ));
    // 120 prompt tokens read in six seconds: the silent part of the turn.
    assert!(matches!(
        events[events.len() - 3],
        ChatEvent::PromptSpeed { tokens: 120, tokens_per_second } if (tokens_per_second - 20.0).abs() < 0.01
    ));
    // 7 tokens in a quarter second.
    assert!(matches!(
        events[events.len() - 2],
        ChatEvent::Throughput { tokens_per_second } if (tokens_per_second - 28.0).abs() < 0.01
    ));
    assert!(matches!(
        events.last(),
        Some(ChatEvent::Done {
            stop_reason: StopReason::EndTurn
        })
    ));
}

/// Ollama reports `done_reason: "stop"` for a turn that asked for tools, so
/// the stop reason has to come from the calls, not from what it says.
#[test]
fn tool_calls_end_the_turn_as_tool_use_with_distinct_ids() {
    let events = drain(&[
        r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"schema","arguments":{"open_tab":"a"}}},{"function":{"name":"schema","arguments":{"open_tab":"b"}}}]},"done":false}"#,
        r#"{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#,
    ]);
    let ids: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ToolCall { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec!["schema-0", "schema-1"]);
    assert!(matches!(
        events.last(),
        Some(ChatEvent::Done {
            stop_reason: StopReason::ToolUse
        })
    ));
}

/// Ids are minted from the turn's running count, not the position in one
/// line: two calls to the same tool on separate lines must not collide, or the
/// agent pairs both results with the first call.
#[test]
fn ids_stay_distinct_across_lines_and_empty_arguments_become_an_object() {
    let events = drain(&[
        r#"{"message":{"role":"assistant","tool_calls":[{"function":{"name":"schema"}}]},"done":false}"#,
        r#"{"message":{"role":"assistant","tool_calls":[{"function":{"name":"schema","arguments":{}}}]},"done":false}"#,
        r#"{"message":{"role":"assistant","content":""},"done":true}"#,
    ]);
    let calls: Vec<(String, Value)> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ToolCall { id, input, .. } => Some((id.clone(), input.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(calls[0].0, "schema-0");
    assert_eq!(calls[1].0, "schema-1");
    // A call with no arguments at all: `null` would fail the tool's own
    // deserialisation where an empty object passes.
    assert_eq!(calls[0].1, json!({}));
}

/// A failure after the 200 arrives as a bare `{"error": ...}` line, which no
/// HTTP status check can catch.
#[test]
fn mid_stream_error_line_surfaces() {
    let events = drain(&[r#"{"error":"no user query found in messages"}"#]);
    assert!(matches!(&events[..], [ChatEvent::Error(m)] if m.contains("no user query")));
}
