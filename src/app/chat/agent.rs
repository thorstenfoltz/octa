//! The agentic turn loop. Runs on its own `std::thread`, holding only the
//! cloned `egui::Context`, the `Arc<Mutex<ChatSessionState>>`, and a moved
//! `ToolContext` of table snapshots - it never borrows `OctaApp` / `TabState`.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use eframe::egui;
use serde_json::Value;

use crate::mcp::tools::ToolContext;

use super::providers::{ChatProvider, ProviderConfig};
use super::session::{ChatSessionState, StreamingTurn, TurnPhase};
use super::types::{ChatEvent, ContentBlock, Message, StopReason, ToolDef};

/// Cap on the size of a single tool result fed back to the model, so one big
/// `read_table` can't blow the context window.
const MAX_TOOL_RESULT_BYTES: usize = 100 * 1024;

/// Everything the worker needs, moved across the thread boundary.
pub struct TurnRequest {
    pub provider: Box<dyn ChatProvider>,
    pub cfg: ProviderConfig,
    pub system: String,
    pub tools: Vec<ToolDef>,
    pub tool_ctx: ToolContext,
    pub max_iterations: usize,
    /// Per-turn cancel flag (also stored on the session as `cancel`). Owning it
    /// per turn means a cancelled+blocked worker can't be "resurrected" when a
    /// later turn resets the session's flag.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Per-turn running flag (also stored on the session as `running`).
    pub running: Arc<std::sync::atomic::AtomicBool>,
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
        tools,
        tool_ctx,
        max_iterations,
        cancel,
        running: _running,
    } = req;

    for _iteration in 0..max_iterations.max(1) {
        if cancel.load(Ordering::Relaxed) {
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

        // Accumulators for this iteration's assistant turn.
        let mut text = String::new();
        let mut tool_calls: Vec<(String, String, Value)> = Vec::new();
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
                    tool_calls.push((id, name, input));
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
            state.lock().unwrap().error = Some(format!("{}: {e}", provider.name()));
            return;
        }
        if let Some(e) = error {
            state.lock().unwrap().error = Some(e);
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        // Commit the assistant turn (prose + tool_use blocks).
        let mut blocks: Vec<ContentBlock> = Vec::new();
        if !text.trim().is_empty() {
            blocks.push(ContentBlock::text(text));
        }
        for (id, name, input) in &tool_calls {
            blocks.push(ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            });
        }
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
        for (id, name, input) in tool_calls {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let (content, is_error) = match super::tools::dispatch(&tool_ctx, &name, input) {
                Ok(v) => (truncate_for_model(&v.to_string()), false),
                Err(e) => (e, true),
            };
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
        // Loop: send the tool results back for the model's next step.
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
