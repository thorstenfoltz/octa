use super::*;
#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
fn text_of(events: &[ChatEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::TextDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

/// Run a shell one-liner as if it were the vendor's program.
#[cfg(unix)]
fn sh(script: &str, prompt: &str) -> (Result<(), String>, Vec<ChatEvent>) {
    let mut events = Vec::new();
    let script = script.to_string();
    let result = turn(
        "sh",
        |_| vec!["-c".to_string(), script.clone()],
        prompt,
        false,
        &AtomicBool::new(false),
        &mut |e| events.push(e),
        None,
    );
    (result, events)
}

#[test]
fn earlier_turns_are_written_out_above_the_newest_message() {
    let messages = vec![
        Message::user_text("make it shorter"),
        Message {
            role: Role::Assistant,
            blocks: vec![ContentBlock::text("Done.")],
        },
        Message::user_text("now in German"),
    ];
    let prompt = flatten("SYSTEM", &messages, &[]);
    let system = prompt.find("SYSTEM").unwrap();
    let first = prompt.find("make it shorter").unwrap();
    let reply = prompt.find("Done.").unwrap();
    let last = prompt.find("now in German").unwrap();
    assert!(system < first && first < reply && reply < last);
    assert!(prompt.contains("[Assistant]"));
}

#[test]
fn a_single_message_needs_no_history_block() {
    let prompt = flatten("SYSTEM", &[Message::user_text("hello")], &[]);
    assert!(!prompt.contains("conversation so far"));
    assert!(prompt.ends_with("hello"));
}

fn tool(name: &str) -> ToolDef {
    ToolDef {
        name: name.into(),
        description: format!("what {name} does"),
        input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
    }
}

/// Feed a reply in pieces and collect what is shown and how it ends.
fn reply_of(pieces: &[&str], with_tools: bool) -> (String, Vec<ChatEvent>) {
    let mut reply = Reply::new(with_tools);
    let mut shown = String::new();
    for piece in pieces {
        if let Some(text) = reply.push(piece) {
            shown.push_str(&text);
        }
    }
    (shown, reply.finish())
}

fn calls_of(events: &[ChatEvent]) -> Vec<(String, Value)> {
    events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ToolCall { name, input, .. } => Some((name.clone(), input.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_prompt_names_every_tool_and_the_block_to_call_it_with() {
    let prompt = flatten(
        "SYSTEM",
        &[Message::user_text("hi")],
        &[tool("run_sql"), tool("profile")],
    );
    assert!(prompt.contains(TOOL_OPEN));
    for name in ["run_sql", "profile"] {
        assert!(prompt.contains(&format!("--- {name} ---")), "{name}");
        assert!(prompt.contains(&format!("what {name} does")), "{name}");
    }
    assert!(prompt.contains("\"query\""), "the schema is in it");
    assert!(prompt.find("=== Tools ===") < prompt.find("message now"));
    assert!(
        prompt.contains("never write one yourself"),
        "no made-up results"
    );
    // Without tools the prompt says nothing about them.
    assert!(!flatten("SYSTEM", &[Message::user_text("hi")], &[]).contains("octa-tool"));
}

#[test]
fn an_earlier_tool_round_is_written_out_with_its_result() {
    let messages = vec![
        Message::user_text("how many rows"),
        Message::assistant(vec![
            ContentBlock::text("I count them."),
            ContentBlock::ToolUse {
                id: "c1".into(),
                name: "count_rows".into(),
                input: serde_json::json!({"open_tab": "@active"}),
            },
        ]),
        Message::tool_results(vec![ContentBlock::ToolResult {
            id: "c1".into(),
            content: "{\"rows\": 42}".into(),
            is_error: false,
        }]),
    ];
    let prompt = flatten("SYSTEM", &messages, &[tool("count_rows")]);
    // The model reads its own call back in the shape it wrote it in.
    let tail = &prompt[prompt.find("conversation so far").unwrap()..];
    let calls = tool_calls_in(tail).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "count_rows");
    assert!(prompt.contains("[Result of count_rows]\n{\"rows\": 42}"));
    assert!(prompt.contains("results of your tool calls"));
    assert!(!prompt.contains("message now"));

    let failed = vec![Message::tool_results(vec![ContentBlock::ToolResult {
        id: "gone".into(),
        content: "no such column".into(),
        is_error: true,
    }])];
    assert!(flatten("S", &failed, &[]).contains("[Result of tool, it FAILED]"));
}

#[test]
fn a_tool_block_becomes_a_call_and_is_not_shown() {
    let (shown, events) = reply_of(
        &[
            "Let me look.\n",
            "~~~~~~octa-tool\n",
            "{\"name\": \"run_sql\", \"arguments\": {\"query\": \"SELECT 1\"}}\n",
            "~~~~~~",
        ],
        true,
    );
    assert_eq!(shown, "Let me look.\n");
    assert_eq!(
        calls_of(&events),
        vec![(
            "run_sql".to_string(),
            serde_json::json!({"query": "SELECT 1"})
        )]
    );
    assert!(matches!(
        events.last(),
        Some(ChatEvent::Done {
            stop_reason: StopReason::ToolUse
        })
    ));
    assert!(!events.iter().any(|e| matches!(e, ChatEvent::TextDelta(_))));
}

#[test]
fn a_block_split_anywhere_is_still_found() {
    // An API-style stream cuts between any two characters.
    let whole = "Sure.\n~~~~~~octa-tool\n{\"name\":\"profile\"}\n~~~~~~\n";
    for cut in 1..whole.len() {
        let (shown, events) = reply_of(&[&whole[..cut], &whole[cut..]], true);
        assert_eq!(shown, "Sure.\n", "cut at {cut}");
        assert_eq!(
            calls_of(&events),
            vec![("profile".to_string(), serde_json::json!({}))],
            "cut at {cut}"
        );
    }
}

#[test]
fn two_blocks_are_two_calls_each_with_its_own_id() {
    let (_, events) = reply_of(
        &["~~~~~~octa-tool\n{\"name\":\"a\"}\n~~~~~~\n~~~~~~octa-tool\n{\"name\":\"b\"}\n~~~~~~"],
        true,
    );
    let ids: Vec<&String> = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ToolCall { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(calls_of(&events)[1].0, "b");
}

#[test]
fn a_block_that_cannot_be_read_is_shown_and_never_run() {
    for broken in [
        // Cut off before the closing line.
        "~~~~~~octa-tool\n{\"name\":\"run_sql\"}",
        // Not JSON.
        "~~~~~~octa-tool\nrun_sql please\n~~~~~~",
        // No name.
        "~~~~~~octa-tool\n{\"arguments\":{}}\n~~~~~~",
        // Arguments that are not an object.
        "~~~~~~octa-tool\n{\"name\":\"a\",\"arguments\":\"x\"}\n~~~~~~",
    ] {
        let (shown, events) = reply_of(&["Hm.\n", broken], true);
        assert!(calls_of(&events).is_empty(), "{broken}");
        let rest: String = events
            .iter()
            .filter_map(|e| match e {
                ChatEvent::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            format!("{shown}{rest}"),
            format!("Hm.\n{broken}"),
            "all of it is shown"
        );
        assert!(matches!(
            events.last(),
            Some(ChatEvent::Done {
                stop_reason: StopReason::EndTurn
            })
        ));
    }
}

#[test]
fn the_marker_inside_a_line_or_without_tools_is_plain_text() {
    let prose = "Write `~~~~~~octa-tool` on its own line.\n~~~~~~ is six tildes.";
    let (shown, events) = reply_of(&[prose], true);
    assert_eq!(shown, prose);
    assert!(calls_of(&events).is_empty());

    // A turn that offered no tools runs none, whatever the reply holds.
    let block = "~~~~~~octa-tool\n{\"name\":\"write_table\"}\n~~~~~~";
    let (shown, events) = reply_of(&[block], false);
    assert_eq!(shown, block);
    assert!(calls_of(&events).is_empty());
}

/// Seen live with Claude Haiku on 2026-11-11: the call without its first
/// and last line, then a result the model made up. The call runs, the
/// invention is never shown.
#[test]
fn a_call_written_without_its_block_still_runs_and_what_follows_is_dropped() {
    let pieces = [
        "{",
        "\"name\":\"count_rows",
        "\",\"arguments\":{\"open_tab\":\"@active\"}}",
        "\n\n{\"rows\": 1284}\n\nThe active tab has 1,284 rows.",
    ];
    let (shown, events) = reply_of(&pieces, true);
    assert_eq!(shown, "", "nothing of it reaches the transcript");
    assert_eq!(
        calls_of(&events),
        vec![(
            "count_rows".to_string(),
            serde_json::json!({"open_tab": "@active"})
        )]
    );
    assert!(!events.iter().any(|e| matches!(e, ChatEvent::TextDelta(_))));

    // JSON that only starts like a call is the model's own text.
    let prose = "{\"name\": \"Ada\", \"age\": 36";
    let (shown, events) = reply_of(&[prose], true);
    let rest: String = events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::TextDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(format!("{shown}{rest}"), prose);
    assert!(calls_of(&events).is_empty());
}

#[test]
fn text_is_shown_as_it_arrives() {
    let mut reply = Reply::new(true);
    assert_eq!(reply.push("Hel").as_deref(), Some("Hel"));
    assert_eq!(reply.push("lo\n").as_deref(), Some("lo\n"));
    // The start of what may become the opening line waits.
    assert_eq!(reply.push("~~~"), None);
    assert_eq!(reply.push("~ strike").as_deref(), Some("~~~~ strike"));
}

/// The whole way: a program that asks for a tool, through `turn`.
#[cfg(unix)]
#[test]
fn a_program_asking_for_a_tool_ends_the_turn_with_that_call() {
    let mut events = Vec::new();
    turn(
        "sh",
        |_| {
            vec![
                "-c".to_string(),
                "cat >/dev/null; printf 'One moment.\\n~~~~~~octa-tool\\n{\"name\":\"count_rows\",\"arguments\":{\"open_tab\":\"@active\"}}\\n~~~~~~\\n'".to_string(),
            ]
        },
        "prompt",
        true,
        &AtomicBool::new(false),
        &mut |e| events.push(e),
        None,
    )
    .unwrap();
    assert_eq!(text_of(&events), "One moment.\n");
    assert_eq!(
        calls_of(&events),
        vec![(
            "count_rows".to_string(),
            serde_json::json!({"open_tab": "@active"})
        )]
    );
    assert!(matches!(
        events.last(),
        Some(ChatEvent::Done {
            stop_reason: StopReason::ToolUse
        })
    ));
}

#[test]
fn the_profiles_path_wins_over_the_default_program() {
    let mut cfg = ProviderConfig {
        model: String::new(),
        base_url: None,
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    };
    assert_eq!(program(&cfg, "codex"), "codex");
    cfg.base_url = Some("  ".into());
    assert_eq!(program(&cfg, "codex"), "codex");
    cfg.base_url = Some("/opt/codex/bin/codex".into());
    assert_eq!(program(&cfg, "codex"), "/opt/codex/bin/codex");
}

#[test]
fn a_program_that_is_not_installed_is_named() {
    let err = run(
        "octa-no-such-program-xyz",
        |_| Vec::new(),
        "hi",
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(err.contains("octa-no-such-program-xyz"), "{err}");
    assert!(err.contains("not found"), "{err}");
}

#[cfg(unix)]
#[test]
fn the_prompt_goes_in_on_stdin_and_the_answer_comes_back() {
    let (result, events) = sh("cat", "line one\nline two");
    result.unwrap();
    assert_eq!(text_of(&events), "line one\nline two");
    assert!(matches!(
        events.last(),
        Some(ChatEvent::Done {
            stop_reason: StopReason::EndTurn
        })
    ));
}

#[cfg(unix)]
#[test]
fn a_prompt_far_longer_than_a_command_line_arrives_whole() {
    let prompt = "x".repeat(400_000);
    let (result, events) = sh("wc -c", &prompt);
    result.unwrap();
    assert_eq!(text_of(&events).trim(), "400000");
}

#[cfg(unix)]
#[test]
fn it_runs_in_an_empty_folder_that_is_gone_afterwards() {
    let (result, events) = sh("pwd; ls -A | wc -l", "");
    result.unwrap();
    let out = text_of(&events);
    let mut lines = out.lines();
    let dir = lines.next().unwrap();
    assert!(dir.contains("octa-assistant-"), "{dir}");
    assert_eq!(lines.next().unwrap().trim(), "0", "the folder is empty");
    assert!(!Path::new(dir).exists(), "and removed after the run");
}

#[cfg(unix)]
#[test]
fn a_failed_run_reports_what_the_program_said() {
    let (result, events) = sh("echo 'Access denied by policy settings' >&2; exit 1", "hi");
    let err = result.unwrap_err();
    assert!(err.contains("Access denied by policy settings"), "{err}");
    assert!(
        !events.iter().any(|e| matches!(e, ChatEvent::Done { .. })),
        "a failed run is not a finished turn"
    );
}

#[cfg(unix)]
#[test]
fn a_run_that_succeeds_without_a_word_is_an_error_not_an_empty_turn() {
    let (result, events) = sh("cat >/dev/null; echo", "hi");
    assert!(result.unwrap_err().contains("without an answer"));
    assert!(events.is_empty());
}

#[cfg(unix)]
#[test]
fn a_silent_failure_still_says_something() {
    let (result, _) = sh("exit 3", "hi");
    assert!(result.unwrap_err().contains("without an answer"));
}

#[cfg(unix)]
#[test]
fn cancelling_stops_a_program_that_is_still_running() {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let result = run(
        "sh",
        |_| vec!["-c".to_string(), "sleep 60".to_string()],
        "hi",
        &cancel,
        &mut |_| {},
    );
    assert!(result.is_ok(), "a cancelled turn is not an error");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "it did not wait for the program"
    );
}

#[test]
fn a_profiles_program_path_is_its_own_and_never_the_global_url() {
    use octa::ui::settings::{AppSettings, ChatProviderKind, chat_profiles};
    let mut settings = AppSettings::default();
    chat_profiles::ensure_profiles(&mut settings);
    let mut profile = chat_profiles::active_profile(&settings).clone();
    profile.kind = ChatProviderKind::CodexCli;
    profile.base_url = String::new();
    let cfg =
        super::super::config_for_profile(&profile, "https://example.test/v1", String::new(), None);
    assert_eq!(cfg.base_url, None, "the endpoint URL is not a program");
    assert_eq!(program(&cfg, "codex"), "codex");

    profile.base_url = " /opt/codex ".into();
    let cfg =
        super::super::config_for_profile(&profile, "https://example.test/v1", String::new(), None);
    assert_eq!(program(&cfg, "codex"), "/opt/codex");
}
