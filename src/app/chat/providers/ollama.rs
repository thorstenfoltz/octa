//! Local Ollama adapter, speaking Ollama's **native** `/api/chat` (streamed as
//! newline-delimited JSON), not the OpenAI-compatible `/v1/chat/completions`
//! this file used to reuse from `openai.rs`.
//!
//! The reason is `num_ctx`. Ollama sizes its default context window from the
//! machine's VRAM - 4k on a modest box (`OLLAMA_CONTEXT_LENGTH` help text:
//! "default: 4k/32k/256k based on VRAM") - and Octa's first request already
//! carries ~6.5k tokens of tool schemas. When the transcript does not fit,
//! Ollama silently drops the **oldest** non-system messages, and in an agent
//! round (`system, user, assistant(tool_calls), tool`) the oldest one is the
//! user's actual question. Models with a built-in renderer then refuse the
//! prompt outright - Qwen 3.8 answers `HTTP 500: no user query found in
//! messages` - and every other model quietly answers a question it can no
//! longer see, which is worse.
//!
//! `num_ctx` is the fix and it has no home in the OpenAI request schema:
//! Ollama's `ChatCompletionRequest` has no field for it and drops unknown keys,
//! so the only way to raise the window per request is the native endpoint.
//!
//! Server lifecycle + model discovery live in `crate::app::chat::ollama`.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, Reasoning, parse_reasoning, stream_lines};

/// Default root URL when none is configured.
pub const DEFAULT_URL: &str = "http://localhost:11434";

/// Context window requested per turn: room for the core tool schemas, a large
/// tool result and an answer, where a VRAM-derived 4k default has room for the
/// schemas alone.
///
/// ponytail: one constant, not a setting. A machine too small for a 32k KV
/// cache pages or runs slow rather than failing, and a tool result big enough
/// to overflow even this still truncates the way it does on every other
/// provider. Give it a Settings box if anyone actually reports needing one.
const NUM_CTX: u64 = 32_768;

/// Time allowed before the first token. Ollama sends no bytes, not even the
/// response headers, until it has loaded the model and evaluated the whole
/// prompt, and on a CPU or a GPU the model does not fit that is minutes for
/// the ~6.5k tokens of tool schemas every turn carries. The hosted 120s cut
/// exactly such turns off with `timeout: receive response`. A local server
/// that accepted the request is working, not gone; the chat panel's Cancel
/// stays usable meanwhile.
const RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

pub struct Ollama;

impl ChatProvider for Ollama {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn stream_turn(
        &self,
        cfg: &ProviderConfig,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        sink: &mut dyn FnMut(ChatEvent),
    ) -> Result<(), String> {
        let base = cfg
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_URL)
            .trim_end_matches('/');
        let endpoint = format!("{base}/api/chat");
        let body = build_body(cfg, system, messages, tools)?;

        // Local Ollama needs no Authorization header.
        let headers: [(&str, String); 0] = [];
        let mut st = StreamState::default();
        stream_lines(
            &endpoint,
            &headers,
            &body,
            RESPONSE_TIMEOUT,
            cancel,
            |line| Ok(handle_line(line, Instant::now(), &mut st, sink)),
        )?;
        if !st.done {
            sink(ChatEvent::Done {
                stop_reason: stop_reason(None, st.tool_calls > 0),
            });
        }
        Ok(())
    }
}

/// What the stream has seen so far. Ollama reports the stop reason and the
/// token counts only on the final line, so the tool-call flag has to survive
/// across lines.
#[derive(Default)]
pub(crate) struct StreamState {
    /// How many tool calls this turn has emitted; also the source of the
    /// minted ids, so it must count across lines and not restart per line.
    tool_calls: usize,
    done: bool,
    /// Tokens streamed so far and when the first one arrived. Ollama emits
    /// one line per generated token (content or thinking), so counting lines
    /// gives a live rate while a slow model is still answering; the final
    /// line's own timing replaces it when the turn ends.
    tokens: u32,
    first_token: Option<Instant>,
}

/// Turn one line of Ollama's newline-delimited JSON into [`ChatEvent`]s.
/// Returns `true` when the stream is finished. Pure apart from `sink`, so the
/// wire format is testable without a server.
pub(crate) fn handle_line(
    line: &str,
    now: Instant,
    st: &mut StreamState,
    sink: &mut dyn FnMut(ChatEvent),
) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    // A mid-stream failure arrives as a plain `{"error": "..."}` line with a
    // 200 already on the wire, so it never reaches the status check in
    // `stream_lines`.
    if let Some(msg) = v["error"].as_str() {
        sink(ChatEvent::Error(msg.to_string()));
        return true;
    }
    let content = v["message"]["content"].as_str().filter(|s| !s.is_empty());
    if let Some(text) = content {
        sink(ChatEvent::TextDelta(text.to_string()));
    }
    // Thinking tokens are not shown, but they are generated at the same
    // rate and count towards it.
    let thinking = v["message"]["thinking"]
        .as_str()
        .is_some_and(|s| !s.is_empty());
    if content.is_some() || thinking {
        st.tokens += 1;
        let first = *st.first_token.get_or_insert(now);
        // The first token marks t0, so n tokens span n - 1 intervals.
        let elapsed = now.duration_since(first).as_secs_f32();
        if st.tokens > 1 && elapsed > 0.0 {
            sink(ChatEvent::Throughput {
                tokens_per_second: (st.tokens - 1) as f32 / elapsed,
            });
        }
    }
    if let Some(calls) = v["message"]["tool_calls"].as_array() {
        for tc in calls {
            let name = tc["function"]["name"].as_str().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            // Ollama assembles tool calls in full before emitting them and
            // only optionally gives them an id; mint one from the turn's
            // running count so two calls to the same tool stay distinct even
            // when they arrive on separate lines.
            let id = match tc["id"].as_str().filter(|s| !s.is_empty()) {
                Some(id) => id.to_string(),
                None => format!("{name}-{}", st.tool_calls),
            };
            st.tool_calls += 1;
            // Arguments are an object here, but a model can emit a call with
            // none at all; `null` would fail the tool's own deserialisation
            // where an empty object deserialises fine.
            let input = tc["function"]["arguments"].clone();
            sink(ChatEvent::ToolCall {
                id,
                name: name.to_string(),
                input: if input.is_object() { input } else { json!({}) },
            });
        }
    }
    if v["done"].as_bool() != Some(true) {
        return false;
    }
    let prompt_eval = v["prompt_eval_count"].as_u64().unwrap_or(0);
    let eval = v["eval_count"].as_u64().unwrap_or(0);
    if prompt_eval > 0 || eval > 0 {
        sink(ChatEvent::Usage {
            input_tokens: prompt_eval as u32,
            output_tokens: eval as u32,
        });
    }
    // How long the silence before the first token was spent: reading the
    // prompt. With Ollama's prompt cache a later round counts only the new
    // tokens, which is still the honest figure for that round's wait.
    let prompt_ns = v["prompt_eval_duration"].as_u64().unwrap_or(0);
    if prompt_eval > 0 && prompt_ns > 0 {
        sink(ChatEvent::PromptSpeed {
            tokens: prompt_eval as u32,
            tokens_per_second: prompt_eval as f32 / (prompt_ns as f32 / 1e9),
        });
    }
    // Generation speed from Ollama's own timing (nanoseconds), the number
    // `ollama run --verbose` prints as "eval rate"; it replaces the live
    // estimate above. An older server without the field keeps the estimate.
    let eval_ns = v["eval_duration"].as_u64().unwrap_or(0);
    if eval > 0 && eval_ns > 0 {
        sink(ChatEvent::Throughput {
            tokens_per_second: eval as f32 / (eval_ns as f32 / 1e9),
        });
    }
    sink(ChatEvent::Done {
        stop_reason: stop_reason(v["done_reason"].as_str(), st.tool_calls > 0),
    });
    st.done = true;
    true
}

/// Ollama reports why generation ended in `done_reason`; a turn that asked for
/// tools is a tool turn whatever it says (it reports `stop` for those).
fn stop_reason(done_reason: Option<&str>, saw_tool_call: bool) -> StopReason {
    if saw_tool_call {
        return StopReason::ToolUse;
    }
    match done_reason {
        None | Some("stop") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        Some(other) => StopReason::Other(other.to_string()),
    }
}

pub(crate) fn build_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Result<Value, String> {
    let mut options = Map::new();
    options.insert("num_ctx".into(), json!(NUM_CTX));
    if let Some(t) = cfg.temperature {
        options.insert("temperature".into(), json!(t));
    }
    // `None` => unlimited: omit the cap so the model uses its own default.
    if let Some(max) = cfg.max_tokens {
        options.insert("num_predict".into(), json!(max));
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    body.insert("messages".into(), json!(messages_to_wire(system, messages)));
    body.insert("stream".into(), json!(true));
    body.insert("options".into(), Value::Object(options));
    if !tools.is_empty() {
        let wire_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();
        body.insert("tools".into(), json!(wire_tools));
    }
    // `think` takes a bool or an effort word here, never a token budget - the
    // native endpoint has no field for one, so say so rather than send a
    // number Ollama will reject with a less obvious message.
    match parse_reasoning(cfg.reasoning.as_deref()) {
        None => {}
        Some(Reasoning::Effort(effort)) => {
            body.insert("think".into(), json!(effort));
        }
        Some(Reasoning::Budget(_)) => {
            return Err(
                "Ollama takes a thinking level (low / medium / high), not a token budget"
                    .to_string(),
            );
        }
    }
    Ok(Value::Object(body))
}

/// Flatten the neutral transcript into Ollama's native message list. Close to
/// the OpenAI shape but not identical: `arguments` is a JSON **object** rather
/// than a string, assistant content is a plain string (never null), and a tool
/// result carries `tool_name` beside `tool_call_id` - the renderers key off the
/// name, and the id alone leaves them nothing to label the result with.
pub(crate) fn messages_to_wire(system: &str, messages: &[Message]) -> Vec<Value> {
    // Tool results only carry the id they answer, so remember which tool each
    // id belonged to as the transcript is walked.
    let mut names: HashMap<&str, &str> = HashMap::new();
    let mut out = Vec::new();
    if !system.is_empty() {
        out.push(json!({ "role": "system", "content": system }));
    }
    for m in messages {
        match m.role {
            Role::System => {
                out.push(json!({ "role": "system", "content": join_text(m) }));
            }
            Role::Assistant => {
                let tool_calls: Vec<Value> = m
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => {
                            names.insert(id.as_str(), name.as_str());
                            Some(json!({
                                "id": id,
                                "function": {
                                    "name": name,
                                    "arguments": if input.is_object() {
                                        input.clone()
                                    } else {
                                        json!({})
                                    },
                                }
                            }))
                        }
                        _ => None,
                    })
                    .collect();
                let mut msg = Map::new();
                msg.insert("role".into(), json!("assistant"));
                msg.insert("content".into(), json!(join_text(m)));
                if !tool_calls.is_empty() {
                    msg.insert("tool_calls".into(), json!(tool_calls));
                }
                out.push(Value::Object(msg));
            }
            Role::User | Role::Tool => {
                let mut had_tool_result = false;
                for b in &m.blocks {
                    if let ContentBlock::ToolResult { id, content, .. } = b {
                        had_tool_result = true;
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "tool_name": names.get(id.as_str()).copied().unwrap_or_default(),
                            "content": content,
                        }));
                    }
                }
                let text = join_text(m);
                if !text.is_empty() || !had_tool_result {
                    out.push(json!({ "role": "user", "content": text }));
                }
            }
        }
    }
    out
}

fn join_text(m: &Message) -> String {
    let mut out = String::new();
    for b in &m.blocks {
        if let ContentBlock::Text { text } = b {
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
#[path = "ollama_tests.rs"]
mod tests;
