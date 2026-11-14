use super::*;

fn cfg(model: &str) -> ProviderConfig {
    ProviderConfig {
        model: model.into(),
        base_url: None,
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    }
}

#[test]
fn the_run_is_read_only_and_reads_the_prompt_from_stdin() {
    let a = args(&cfg(""), Path::new("/tmp/x"));
    assert_eq!(a.first().map(String::as_str), Some("exec"));
    assert_eq!(a.last().map(String::as_str), Some("-"), "prompt on stdin");
    let sandbox = a.iter().position(|s| s == "--sandbox").unwrap();
    assert_eq!(a[sandbox + 1], "read-only");
    let dir = a.iter().position(|s| s == "-C").unwrap();
    assert_eq!(a[dir + 1], "/tmp/x");
    assert!(a.iter().any(|s| s == "--ignore-user-config"));
    assert!(!a.iter().any(|s| s == "-m"), "no model, no model option");
    assert!(
        !a.iter().any(|s| s.contains("dangerously")),
        "never a bypass option"
    );
}

#[test]
fn a_model_named_in_the_profile_is_passed_on() {
    let a = args(&cfg("gpt-6-luna"), Path::new("/tmp/x"));
    let m = a.iter().position(|s| s == "-m").unwrap();
    assert_eq!(a[m + 1], "gpt-6-luna");
}

/// Talks to the real service on the signed-in account, so it only runs when
/// asked for: `OCTA_TEST_CODEX_CLI=1 cargo test --bin octa codex_cli -- --ignored`.
#[test]
#[ignore = "needs a signed-in Codex CLI and spends plan allowance"]
fn the_real_program_answers_and_asks_for_a_tool() {
    if std::env::var("OCTA_TEST_CODEX_CLI").is_err() {
        return;
    }
    let ask = |question: &str, tools: &[ToolDef]| {
        let mut events = Vec::new();
        CodexCli
            .stream_turn(
                &cfg(""),
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
