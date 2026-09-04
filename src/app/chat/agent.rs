//! The agentic turn loop. Runs on its own `std::thread`, holding only the
//! cloned `egui::Context`, the `Arc<Mutex<ChatSessionState>>`, and a moved
//! `ToolContext` of table snapshots - it never borrows `OctaApp` / `TabState`.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use eframe::egui;
use serde_json::Value;

use crate::mcp::tools::ToolContext;

use super::providers::{ChatProvider, ProviderConfig};
use super::session::{ChatSessionState, StreamingTurn, TurnPhase};
use super::tool_groups::ToolGroup;
use super::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

/// Cap on the size of a single tool result fed back to the model, so one big
/// `read_table` can't blow the context window.
const MAX_TOOL_RESULT_BYTES: usize = 100 * 1024;

/// Everything the worker needs, moved across the thread boundary.
pub struct TurnRequest {
    pub provider: Box<dyn ChatProvider>,
    pub cfg: ProviderConfig,
    pub system: String,
    /// What goes out with the first request: the core group plus the
    /// `enable_tools` menu. The loop grows this list when the model fetches a
    /// group (see `chat::tool_groups`).
    pub tools: Vec<ToolDef>,
    /// Whether this profile may use the write tools, and which tools the user
    /// switched off. Both are needed here because a mid-turn fetch has to
    /// apply the same filtering the first request did.
    pub allow_writes: bool,
    pub disabled_tools: BTreeSet<String>,
    pub tool_ctx: ToolContext,
    pub max_iterations: usize,
    /// Per-turn cancel flag (also stored on the session as `cancel`). Owning it
    /// per turn means a cancelled+blocked worker can't be "resurrected" when a
    /// later turn resets the session's flag.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Per-turn running flag (also stored on the session as `running`).
    pub running: Arc<std::sync::atomic::AtomicBool>,
    /// `Some(session_id)` when the tool-call audit log is enabled; each tool
    /// call is then appended to `<config_dir>/chat_audit/<id>.jsonl`. `None`
    /// disables auditing for the turn.
    pub audit_session: Option<String>,
}

/// Spawn the worker thread for one user turn. The caller has already appended
/// the user's `Message` and installed fresh per-turn `running`/`cancel` flags.
pub fn spawn_turn(state: Arc<Mutex<ChatSessionState>>, req: TurnRequest, egui_ctx: egui::Context) {
    let running = req.running.clone();
    std::thread::spawn(move || {
        run_turn(&state, req, &egui_ctx);
        // Release our own running flag. Only clear the shared streaming scratch
        // if we are still the active turn - a newer turn may have replaced the
        // flags while this (cancelled) worker was blocked on a network read.
        running.store(false, Ordering::Relaxed);
        {
            let mut s = state.lock().unwrap();
            if Arc::ptr_eq(&s.running, &running) {
                s.streaming = None;
                s.running.store(false, Ordering::Relaxed);
                s.refresh_auto_title();
            }
        }
        egui_ctx.request_repaint();
    });
}

fn run_turn(state: &Arc<Mutex<ChatSessionState>>, req: TurnRequest, ctx: &egui::Context) {
    let TurnRequest {
        provider,
        cfg,
        system,
        mut tools,
        allow_writes,
        disabled_tools,
        tool_ctx,
        max_iterations,
        cancel,
        running,
        audit_session,
    } = req;

    // Fetching a group of tools is not work, so it must not spend the user's
    // tool-iteration budget - with the default of 3, one fetch would eat a
    // third of it. It is still bounded: after this many fetch-only rounds a
    // model that keeps asking instead of answering starts paying like any
    // other round.
    let max_fetch_rounds = ToolGroup::ALL.len();
    let mut fetch_rounds = 0usize;
    let mut rounds = 0usize;

    while rounds < max_iterations.max(1) {
        if cancel.load(Ordering::Relaxed) {
            answer_cancelled_tool_calls(state);
            return;
        }

        // Snapshot the transcript for this provider call.
        let messages: Vec<Message> = state.lock().unwrap().messages.clone();

        // Reset the live streaming scratch for this iteration.
        {
            let mut s = state.lock().unwrap();
            s.streaming = Some(StreamingTurn {
                phase: TurnPhase::Streaming,
                ..Default::default()
            });
        }
        ctx.request_repaint();

        // Accumulators for this iteration's assistant turn. `raw_blocks` keeps
        // tool calls and provider-specific items (e.g. OpenAI Responses
        // reasoning) in event order, so a replay to the provider preserves the
        // ordering it produced.
        let mut text = String::new();
        let mut raw_blocks: Vec<ContentBlock> = Vec::new();
        let mut error: Option<String> = None;
        let mut stop = StopReason::EndTurn;

        let stream_result = {
            let mut sink = |ev: ChatEvent| match ev {
                ChatEvent::TextDelta(d) => {
                    text.push_str(&d);
                    if let Ok(mut s) = state.lock()
                        && let Some(st) = &mut s.streaming
                    {
                        st.text = text.clone();
                    }
                    ctx.request_repaint();
                }
                ChatEvent::ToolCall { id, name, input } => {
                    raw_blocks.push(ContentBlock::ToolUse { id, name, input });
                }
                ChatEvent::ProviderData(data) => {
                    raw_blocks.push(ContentBlock::ProviderData {
                        provider: provider.name().to_string(),
                        data,
                    });
                }
                ChatEvent::Usage {
                    input_tokens,
                    output_tokens,
                } => {
                    if let Ok(mut s) = state.lock() {
                        s.input_tokens = s.input_tokens.saturating_add(input_tokens);
                        s.output_tokens = s.output_tokens.saturating_add(output_tokens);
                    }
                }
                ChatEvent::Done { stop_reason } => stop = stop_reason,
                ChatEvent::Error(e) => error = Some(e),
            };
            provider.stream_turn(&cfg, &system, &messages, &tools, &cancel, &mut sink)
        };

        if let Err(e) = stream_result {
            set_error_if_current(state, &running, format!("{}: {e}", provider.name()));
            return;
        }
        if let Some(e) = error {
            set_error_if_current(state, &running, e);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            answer_cancelled_tool_calls(state);
            return;
        }

        // Commit the assistant turn (prose + tool_use / provider blocks, in
        // the order the provider emitted them).
        let tool_calls: Vec<(String, String, Value)> = raw_blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();
        let mut blocks: Vec<ContentBlock> = Vec::new();
        if !text.trim().is_empty() {
            blocks.push(ContentBlock::text(text));
        }
        blocks.extend(raw_blocks);
        if !blocks.is_empty() {
            state
                .lock()
                .unwrap()
                .messages
                .push(Message::assistant(blocks));
        }

        // No tools requested -> the turn is done (whatever the stop reason).
        let _ = &stop;
        if tool_calls.is_empty() {
            return;
        }

        // Execute the requested tools and feed results back.
        {
            let mut s = state.lock().unwrap();
            if let Some(st) = &mut s.streaming {
                st.phase = TurnPhase::ExecutingTools;
                st.pending_tool_count = tool_calls.len();
            }
        }
        ctx.request_repaint();

        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        // A round that only fetched tools is free (see `max_fetch_rounds`). A
        // fetch that loaded nothing counts as work, so a model looping on the
        // same group cannot spin forever.
        let mut did_real_work = false;
        for (id, name, input) in tool_calls {
            if cancel.load(Ordering::Relaxed) {
                answer_cancelled_tool_calls(state);
                return;
            }
            let args_bytes = input.to_string().len();
            let started = std::time::Instant::now();
            // `enable_tools` is answered here rather than in `tools::dispatch`,
            // because the whole point of it is to change the tool list this
            // turn is running with, which dispatch cannot reach.
            let (content, is_error) = if name == super::tools::ENABLE_TOOLS {
                let (added, msg) =
                    super::tools::enable_groups(&input, allow_writes, &disabled_tools, &tools);
                did_real_work |= added.is_empty();
                tools.extend(added);
                (msg, false)
            } else if !tools.iter().any(|d| d.name == name) {
                // Either the user switched this tool off, or the model called
                // it from memory without fetching its group. Say which, rather
                // than running a tool that is not on the table.
                did_real_work = true;
                let hint = if disabled_tools.contains(&name) {
                    format!(
                        "the tool `{name}` is switched off for this install \
                         (Settings > Chat / Assistant > Tools). Tell the user that, \
                         and use another tool if one fits."
                    )
                } else {
                    match ToolGroup::of(&name) {
                        Some(group) => format!(
                            "the tool `{name}` is not loaded: call `enable_tools` with group \"{}\" first",
                            group.id()
                        ),
                        None => format!("unknown tool: {name}"),
                    }
                };
                (hint, true)
            } else {
                did_real_work = true;
                match super::tools::dispatch(&tool_ctx, &name, input) {
                    Ok(v) => (truncate_for_model(&v.to_string()), false),
                    Err(e) => (e, true),
                }
            };
            // Audit log (opt-in): one JSON line per tool call.
            if let Some(ref sid) = audit_session {
                super::audit::record(
                    sid,
                    &super::audit::AuditEntry {
                        ts_unix: super::audit::now_unix(),
                        tool: name.clone(),
                        args_bytes,
                        result_bytes: content.len(),
                        duration_ms: started.elapsed().as_millis(),
                        is_error,
                    },
                );
            }
            ctx.request_repaint();

            result_blocks.push(ContentBlock::ToolResult {
                id,
                content,
                is_error,
            });
        }

        state
            .lock()
            .unwrap()
            .messages
            .push(Message::tool_results(result_blocks));

        if did_real_work || fetch_rounds >= max_fetch_rounds {
            rounds += 1;
        } else {
            fetch_rounds += 1;
        }
        // Loop: send the tool results back for the model's next step.
    }
}

/// Answer any tool call the cancel left hanging.
///
/// The assistant message carrying `tool_use` is already in the transcript by
/// the time the user hits Cancel, and providers reject a replay where a
/// `tool_use` has no matching `tool_result`. The session was therefore broken
/// for good: every later turn failed with the same error and only "New chat"
/// recovered it. One synthetic result per outstanding call keeps the
/// transcript valid, and says plainly what happened.
fn answer_cancelled_tool_calls(state: &Arc<Mutex<ChatSessionState>>) {
    let Ok(mut s) = state.lock() else {
        return;
    };
    let Some(last) = s.messages.last() else {
        return;
    };
    if last.role != Role::Assistant {
        return;
    }
    let pending: Vec<ContentBlock> = last
        .blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(ContentBlock::ToolResult {
                id: id.clone(),
                content: "Cancelled by the user.".to_string(),
                is_error: true,
            }),
            _ => None,
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    s.messages.push(Message::tool_results(pending));
}

/// Report an error only while this worker is still the active turn: a
/// cancelled worker's late network failure must not land as a banner on the
/// turn that replaced it.
fn set_error_if_current(
    state: &Arc<Mutex<ChatSessionState>>,
    running: &Arc<std::sync::atomic::AtomicBool>,
    message: String,
) {
    if let Ok(mut s) = state.lock()
        && Arc::ptr_eq(&s.running, running)
    {
        s.error = Some(message);
    }
}

/// Cap a tool result so a single oversized payload doesn't overflow context.
fn truncate_for_model(s: &str) -> String {
    if s.len() <= MAX_TOOL_RESULT_BYTES {
        return s.to_string();
    }
    let mut cut = MAX_TOOL_RESULT_BYTES;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}\n[truncated: {} of {} bytes shown. Narrow the query (e.g. add a LIMIT or select \
fewer columns via run_sql) to see the rest.]",
        &s[..cut],
        cut,
        s.len()
    )
}

#[cfg(test)]
mod tests {
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
}
