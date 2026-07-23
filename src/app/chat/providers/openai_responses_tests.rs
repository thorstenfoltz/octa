//! Unit tests for the OpenAI Responses driver. Split out of the source file;
//! included back via `#[path]` so it stays an inner `tests` module with
//! access to the parent module's private items.

use super::*;
use serde_json::json;

fn cfg(reasoning: Option<&str>, max_tokens: Option<usize>) -> ProviderConfig {
    ProviderConfig {
        model: "gpt-5.5".into(),
        base_url: None,
        api_key: "k".into(),
        temperature: 0.4,
        max_tokens,
        reasoning: reasoning.map(str::to_string),
    }
}

fn one_tool() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "read_table".into(),
        description: "read".into(),
        input_schema: json!({ "type": "object", "properties": {} }),
    }]
}

#[test]
fn body_uses_flat_tools_with_strict_false() {
    let body = build_responses_body(&cfg(None, None), "sys", &[], &one_tool()).unwrap();
    let t = &body["tools"][0];
    // Flat: name at the top level, no nested "function" object, and strict
    // must be explicit false (the API defaults to strict schemas).
    assert_eq!(t["type"], "function");
    assert_eq!(t["name"], "read_table");
    assert!(t.get("function").is_none());
    assert_eq!(t["strict"], json!(false));
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["instructions"], "sys");
    assert_eq!(body["store"], json!(false));
    assert_eq!(body["stream"], json!(true));
}

#[test]
fn reasoning_word_sets_effort_and_omits_temperature() {
    let body = build_responses_body(&cfg(Some("high"), Some(9000)), "", &[], &[]).unwrap();
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert!(body.get("temperature").is_none());
    assert_eq!(body["max_output_tokens"], json!(9000));
    assert!(body.get("instructions").is_none(), "empty system omitted");
}

#[test]
fn numeric_reasoning_is_a_local_error() {
    let err = build_responses_body(&cfg(Some("8000"), None), "", &[], &[]).unwrap_err();
    assert!(err.contains("effort word"), "{err}");
}

#[test]
fn no_reasoning_keeps_temperature_and_skips_include() {
    let body = build_responses_body(&cfg(None, None), "", &[], &[]).unwrap();
    let temp = body["temperature"].as_f64().expect("temperature present");
    assert!((temp - 0.4).abs() < 1e-6, "{temp}");
    assert!(body.get("reasoning").is_none());
    assert!(body.get("include").is_none());
    assert!(body.get("max_output_tokens").is_none(), "None cap omitted");
}

#[test]
fn input_maps_tool_use_and_result_with_matching_call_id() {
    let messages = vec![
        Message::user_text("insert a row"),
        Message::assistant(vec![
            ContentBlock::ProviderData {
                provider: "openai".into(),
                data: json!({ "type": "reasoning", "id": "rs_1", "encrypted_content": "opaque" }),
            },
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "edit_table".into(),
                input: json!({ "path": "f.csv" }),
            },
        ]),
        Message::tool_results(vec![ContentBlock::ToolResult {
            id: "call_1".into(),
            content: "{\"ok\":true}".into(),
            is_error: false,
        }]),
    ];
    let input = messages_to_input(&messages);

    assert_eq!(input[0]["role"], "user");
    // The reasoning item replays verbatim BEFORE its turn's function_call.
    assert_eq!(input[1]["type"], "reasoning");
    assert_eq!(input[1]["encrypted_content"], "opaque");
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input[2]["call_id"], "call_1");
    assert_eq!(input[2]["name"], "edit_table");
    // Arguments are a JSON *string* on this wire.
    assert_eq!(input[2]["arguments"], "{\"path\":\"f.csv\"}");
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[3]["call_id"], "call_1");
}

#[test]
fn foreign_provider_data_is_not_replayed() {
    let messages = vec![Message::assistant(vec![ContentBlock::ProviderData {
        provider: "anthropic".into(),
        data: json!({ "type": "thinking" }),
    }])];
    assert!(messages_to_input(&messages).is_empty());
}

/// Drive a scripted event sequence through `handle_event` and collect what
/// the sink saw.
fn run_events(events: &[Value]) -> (Vec<ChatEvent>, bool) {
    let mut st = RespState::default();
    let mut seen = Vec::new();
    let mut done = false;
    for ev in events {
        done = handle_event(&mut st, ev, &mut |e| seen.push(e));
        if done {
            break;
        }
    }
    (seen, done)
}

#[test]
fn event_sequence_streams_text_tools_usage_and_done() {
    let (seen, done) = run_events(&[
        json!({ "type": "response.output_text.delta", "delta": "Hel" }),
        json!({ "type": "response.output_text.delta", "delta": "lo" }),
        json!({ "type": "response.output_item.done", "item": {
            "type": "reasoning", "id": "rs_1", "encrypted_content": "opaque" } }),
        json!({ "type": "response.output_item.done", "item": {
            "type": "function_call", "call_id": "call_9",
            "name": "run_sql", "arguments": "{\"query\":\"SELECT 1\"}" } }),
        json!({ "type": "response.completed", "response": {
            "usage": { "input_tokens": 12, "output_tokens": 34 } } }),
    ]);
    assert!(done);

    let text: String = seen
        .iter()
        .filter_map(|e| match e {
            ChatEvent::TextDelta(d) => Some(d.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello");

    let call = seen
        .iter()
        .find_map(|e| match e {
            ChatEvent::ToolCall { id, name, input } => Some((id, name, input)),
            _ => None,
        })
        .expect("one assembled tool call");
    assert_eq!(call.0, "call_9");
    assert_eq!(call.1, "run_sql");
    assert_eq!(call.2["query"], "SELECT 1");

    assert!(seen.iter().any(|e| matches!(e,
        ChatEvent::ProviderData(v) if v["type"] == "reasoning")));
    assert!(seen.iter().any(|e| matches!(
        e,
        ChatEvent::Usage {
            input_tokens: 12,
            output_tokens: 34
        }
    )));
    assert!(seen.iter().any(|e| matches!(
        e,
        ChatEvent::Done {
            stop_reason: StopReason::ToolUse
        }
    )));
}

#[test]
fn completed_without_tool_calls_ends_the_turn() {
    let (seen, done) = run_events(&[json!({ "type": "response.completed", "response": {} })]);
    assert!(done);
    assert!(seen.iter().any(|e| matches!(
        e,
        ChatEvent::Done {
            stop_reason: StopReason::EndTurn
        }
    )));
}

#[test]
fn incomplete_maps_max_output_tokens_to_max_tokens() {
    let (seen, done) = run_events(&[json!({ "type": "response.incomplete", "response": {
        "incomplete_details": { "reason": "max_output_tokens" } } })]);
    assert!(done);
    assert!(seen.iter().any(|e| matches!(
        e,
        ChatEvent::Done {
            stop_reason: StopReason::MaxTokens
        }
    )));
}

#[test]
fn failed_and_error_events_surface_the_message() {
    let (seen, done) = run_events(&[json!({ "type": "response.failed", "response": {
        "error": { "message": "boom" } } })]);
    assert!(done);
    assert!(
        seen.iter()
            .any(|e| matches!(e, ChatEvent::Error(m) if m == "boom"))
    );

    let (seen, done) = run_events(&[json!({ "type": "error", "message": "bad stream" })]);
    assert!(done);
    assert!(
        seen.iter()
            .any(|e| matches!(e, ChatEvent::Error(m) if m == "bad stream"))
    );
}
