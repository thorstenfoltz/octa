use super::*;

fn session_with_pending_tool_call() -> Arc<Mutex<ChatSessionState>> {
    let mut s = ChatSessionState::new("anthropic", "some-model");
    s.messages.push(Message::user_text("summarise the table"));
    s.messages
        .push(Message::assistant(vec![ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "read_table".into(),
            input: serde_json::json!({}),
        }]));
    Arc::new(Mutex::new(s))
}

#[test]
fn cancelling_before_a_tool_ran_leaves_a_replayable_transcript() {
    let state = session_with_pending_tool_call();
    answer_cancelled_tool_calls(&state);

    let s = state.lock().unwrap();
    let last = s.messages.last().expect("a result message was appended");
    match last.blocks.as_slice() {
        [ContentBlock::ToolResult { id, is_error, .. }] => {
            assert_eq!(id, "call_1", "the result must answer the call");
            assert!(is_error, "a cancelled call is not a success");
        }
        other => panic!("expected one tool result, got {other:?}"),
    }
}

#[test]
fn a_transcript_with_nothing_outstanding_is_left_alone() {
    let mut s = ChatSessionState::new("anthropic", "some-model");
    s.messages.push(Message::user_text("hello"));
    let state = Arc::new(Mutex::new(s));
    answer_cancelled_tool_calls(&state);
    assert_eq!(state.lock().unwrap().messages.len(), 1);
}

/// A provider that plays back one list of events per request.
struct Scripted(Mutex<Vec<Vec<ChatEvent>>>);

impl ChatProvider for Scripted {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn stream_turn(
        &self,
        _cfg: &ProviderConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[ToolDef],
        _cancel: &std::sync::atomic::AtomicBool,
        sink: &mut dyn FnMut(ChatEvent),
    ) -> Result<(), String> {
        for event in self.0.lock().unwrap().remove(0) {
            sink(event);
        }
        Ok(())
    }
}

/// The debug box's data: each request is its own step, with its own tokens, and
/// the tool it called and what came back.
#[test]
fn every_request_to_the_model_becomes_its_own_trace_step() {
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut session = ChatSessionState::new("scripted", "some-model");
    session.running = running.clone();
    session.messages.push(Message::user_text("how many rows?"));
    let state = Arc::new(Mutex::new(session));
    let usage = |input_tokens, output_tokens| ChatEvent::Usage {
        input_tokens,
        output_tokens,
    };
    let provider = Scripted(Mutex::new(vec![
        vec![
            usage(100, 0),
            ChatEvent::ToolCall {
                id: "1".into(),
                name: "schema".into(),
                input: serde_json::json!({ "open_tab": "#1" }),
            },
            usage(0, 7),
        ],
        vec![usage(150, 5), ChatEvent::TextDelta("891 rows".into())],
    ]));
    let request = TurnRequest {
        provider: Box::new(provider),
        cfg: ProviderConfig {
            model: "some-model".into(),
            base_url: None,
            api_key: String::new(),
            temperature: None,
            max_tokens: None,
            reasoning: None,
            verbosity: None,
            pro_mode: false,
        },
        system: "you are a test".into(),
        tools: Vec::new(),
        allow_writes: false,
        disabled_tools: Default::default(),
        tool_ctx: ToolContext::for_mcp(None, 0, false, false, vec![], true, 0),
        max_iterations: 3,
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        running,
        audit_session: None,
    };
    run_turn(&state, request, &egui::Context::default());

    let s = state.lock().unwrap();
    assert_eq!(s.trace.len(), 2);
    let (first, second) = (&s.trace[0], &s.trace[1]);
    assert_eq!((first.input_tokens, first.output_tokens), (100, 7));
    assert_eq!((second.input_tokens, second.output_tokens), (150, 5));
    assert_eq!(
        (s.input_tokens, s.output_tokens),
        (250, 12),
        "the total still adds up"
    );

    assert_eq!(first.system, "you are a test");
    assert_eq!(first.model, "some-model");
    assert!(first.sent.contains("how many rows?"), "{}", first.sent);
    assert!(first.millis.is_some());

    assert_eq!(first.tool_runs.len(), 1);
    assert_eq!(first.tool_runs[0].name, "schema");
    assert!(first.tool_runs[0].input.contains("open_tab"));
    assert!(first.tool_runs[0].is_error, "not offered, so refused");
    assert!(!first.tool_runs[0].output.is_empty());

    // Only what is new: the question is not shown a second time.
    assert!(!second.sent.contains("how many rows?"), "{}", second.sent);
    assert!(second.sent.contains("schema"), "{}", second.sent);
    assert_eq!(second.reply, "891 rows");
}

#[test]
fn a_long_session_keeps_only_the_last_trace_steps() {
    let mut s = ChatSessionState::new("scripted", "some-model");
    for i in 0..205 {
        s.push_trace(TraceStep {
            model: i.to_string(),
            ..Default::default()
        });
    }
    assert_eq!(s.trace.len(), 200);
    assert_eq!(s.trace[0].model, "5");
}

/// A turn with `Scripted` events and no tools on offer, so every tool call fails.
fn run_scripted(
    rounds: Vec<Vec<ChatEvent>>,
    max_iterations: usize,
) -> Arc<Mutex<ChatSessionState>> {
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut session = ChatSessionState::new("scripted", "some-model");
    session.running = running.clone();
    session.messages.push(Message::user_text("add a column"));
    let state = Arc::new(Mutex::new(session));
    let request = TurnRequest {
        provider: Box::new(Scripted(Mutex::new(rounds))),
        cfg: ProviderConfig {
            model: "some-model".into(),
            base_url: None,
            api_key: String::new(),
            temperature: None,
            max_tokens: None,
            reasoning: None,
            verbosity: None,
            pro_mode: false,
        },
        system: "you are a test".into(),
        tools: Vec::new(),
        allow_writes: false,
        disabled_tools: Default::default(),
        tool_ctx: ToolContext::for_mcp(None, 0, false, false, vec![], true, 0),
        max_iterations,
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        running,
        audit_session: None,
    };
    run_turn(&state, request, &egui::Context::default());
    state
}

fn call(name: &str) -> ChatEvent {
    ChatEvent::ToolCall {
        id: "1".into(),
        name: name.into(),
        input: serde_json::json!({ "ops": [{ "action": "add_column" }] }),
    }
}

/// The report: the same error four times, then "I've added a new column". The
/// model gets no say after the third, and the 4th script entry is never read.
#[test]
fn the_same_failure_three_times_ends_the_turn_with_the_truth() {
    let lie = ChatEvent::TextDelta("I've added the column".into());
    let state = run_scripted(
        vec![
            vec![call("edit_open_tab")],
            vec![call("edit_open_tab")],
            vec![call("edit_open_tab")],
            vec![lie],
        ],
        12,
    );
    let s = state.lock().unwrap();
    assert_eq!(s.trace.len(), 3, "no fourth request");
    let banner = s.error.as_deref().expect("a banner says what happened");
    assert!(
        banner.contains("edit_open_tab") && banner.contains("3"),
        "{banner}"
    );
    let text: String = s
        .messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect();
    assert!(!text.contains("I've added"), "{text}");
    // Every call has its answer, so the next message can still be sent.
    assert_eq!(s.messages.last().unwrap().role, Role::User);
}

#[test]
fn different_failures_do_not_count_as_the_same_one() {
    let state = run_scripted(
        vec![
            vec![call("a")],
            vec![call("b")],
            vec![call("a")],
            vec![ChatEvent::TextDelta("sorry".into())],
        ],
        12,
    );
    let s = state.lock().unwrap();
    assert_eq!(s.trace.len(), 4, "the model got to answer");
}

/// One failed call, then a reply: the reply may be honest or not, Octa cannot
/// tell, so it says what it knows.
#[test]
fn a_reply_right_after_a_failed_call_carries_a_warning() {
    let state = run_scripted(
        vec![
            vec![call("edit_open_tab")],
            vec![ChatEvent::TextDelta("done!".into())],
        ],
        12,
    );
    let s = state.lock().unwrap();
    assert_eq!(
        s.error.as_deref(),
        Some(octa::i18n::t("chat.last_tool_failed").as_str())
    );
}

#[test]
fn a_reply_without_any_failure_carries_no_warning() {
    let state = run_scripted(vec![vec![ChatEvent::TextDelta("hello".into())]], 12);
    assert!(state.lock().unwrap().error.is_none());
}
