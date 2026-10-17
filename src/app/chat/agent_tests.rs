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
