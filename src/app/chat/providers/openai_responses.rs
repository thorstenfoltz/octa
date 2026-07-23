//! OpenAI Responses API driver (`/v1/responses`). OpenAI proper streams
//! through here: gpt-5.x refuses `reasoning_effort` together with function
//! tools on the legacy Chat Completions endpoint, and the Responses API is
//! where reasoning + tools work together. The OpenAI-compatible and Ollama
//! providers stay on Chat Completions (that is what "compatible" means).
//!
//! Statelessness: we send `store: false` and ask for
//! `reasoning.encrypted_content`, capture the reasoning items the model
//! emits, and replay them verbatim before their turn's function_call items -
//! gpt-5.x rejects a replayed function_call whose paired reasoning item is
//! missing.

use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ProviderConfig, stream_sse};

const ENDPOINT: &str = "https://api.openai.com/v1/responses";

/// Blocking streaming driver: build the body, POST, hand each SSE payload to
/// [`handle_event`].
pub(crate) fn run_responses(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ChatEvent),
) -> Result<(), String> {
    let body = build_responses_body(cfg, system, messages, tools)?;
    let headers = [("authorization", format!("Bearer {}", cfg.api_key))];
    let mut st = RespState::default();
    stream_sse(ENDPOINT, &headers, &body, cancel, |data| {
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        Ok(handle_event(&mut st, &v, sink))
    })
}

/// Per-stream state: whether any function_call was seen (decides the final
/// stop reason) and whether a terminal event already fired.
#[derive(Default)]
pub(crate) struct RespState {
    saw_tool_call: bool,
}

/// Process one parsed SSE event; returns `true` when the stream is finished.
/// The Responses API has no `[DONE]` sentinel - every payload self-describes
/// via `type`, and `response.completed` / `.incomplete` / `.failed` terminate.
pub(crate) fn handle_event(st: &mut RespState, v: &Value, sink: &mut dyn FnMut(ChatEvent)) -> bool {
    match v["type"].as_str().unwrap_or("") {
        "response.output_text.delta" => {
            if let Some(d) = v["delta"].as_str()
                && !d.is_empty()
            {
                sink(ChatEvent::TextDelta(d.to_string()));
            }
            false
        }
        // Arguments arrive complete in the item.done event, so the
        // function_call_arguments.delta events need no accumulator at all.
        "response.output_item.done" => {
            let item = &v["item"];
            match item["type"].as_str().unwrap_or("") {
                "function_call" => {
                    st.saw_tool_call = true;
                    let args = item["arguments"].as_str().unwrap_or("");
                    let input = if args.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(args).unwrap_or(json!({}))
                    };
                    sink(ChatEvent::ToolCall {
                        id: item["call_id"].as_str().unwrap_or("").to_string(),
                        name: item["name"].as_str().unwrap_or("").to_string(),
                        input,
                    });
                }
                "reasoning" => {
                    // Carries encrypted_content (we asked via `include`);
                    // replayed verbatim on the next request.
                    sink(ChatEvent::ProviderData(item.clone()));
                }
                _ => {}
            }
            false
        }
        "response.completed" => {
            let usage = &v["response"]["usage"];
            if !usage.is_null() {
                sink(ChatEvent::Usage {
                    input_tokens: usage["input_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: usage["output_tokens"].as_u64().unwrap_or(0) as u32,
                });
            }
            sink(ChatEvent::Done {
                stop_reason: if st.saw_tool_call {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                },
            });
            true
        }
        "response.incomplete" => {
            let reason = v["response"]["incomplete_details"]["reason"]
                .as_str()
                .unwrap_or("incomplete");
            sink(ChatEvent::Done {
                stop_reason: if reason == "max_output_tokens" {
                    StopReason::MaxTokens
                } else {
                    StopReason::Other(reason.to_string())
                },
            });
            true
        }
        "response.failed" => {
            let msg = v["response"]["error"]["message"]
                .as_str()
                .unwrap_or("response failed")
                .to_string();
            sink(ChatEvent::Error(msg));
            true
        }
        "error" => {
            let msg = v["message"].as_str().unwrap_or("stream error").to_string();
            sink(ChatEvent::Error(msg));
            true
        }
        _ => false,
    }
}

/// The Responses request body. Fallible (mirrors anthropic's `build_body`):
/// a numeric reasoning value is a local error before any network round trip.
pub(crate) fn build_responses_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Result<Value, String> {
    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    body.insert("stream".into(), json!(true));
    // Stateless: nothing persisted server-side; reasoning items round-trip
    // through the transcript instead (see `include` below).
    body.insert("store".into(), json!(false));
    if !system.is_empty() {
        body.insert("instructions".into(), json!(system));
    }
    body.insert("input".into(), json!(messages_to_input(messages)));

    // Responses function tools are FLAT (no nested "function" object), and
    // `strict` must be explicit false: the API defaults to strict mode, which
    // our schemars-derived schemas do not satisfy.
    let wire_tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": t.input_schema,
                "strict": false,
            })
        })
        .collect();
    if !wire_tools.is_empty() {
        body.insert("tools".into(), json!(wire_tools));
        body.insert("tool_choice".into(), json!("auto"));
    }

    if let Some(max) = cfg.max_tokens {
        body.insert("max_output_tokens".into(), json!(max));
    }

    let reasoning = cfg
        .reasoning
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(effort) = reasoning {
        if effort.parse::<i64>().is_ok() {
            return Err(
                "OpenAI takes a reasoning effort word (minimal / low / medium / high), \
                 not a token budget"
                    .to_string(),
            );
        }
        // Passed verbatim so new effort values keep working without a release.
        body.insert("reasoning".into(), json!({ "effort": effort }));
        body.insert("include".into(), json!(["reasoning.encrypted_content"]));
        // Reasoning models reject temperature; omit it entirely.
    } else {
        body.insert("temperature".into(), json!(cfg.temperature));
    }

    Ok(Value::Object(body))
}

/// Flatten the neutral transcript into Responses `input` items. Reasoning
/// items (`ProviderData`) were captured in event order, i.e. before their
/// turn's function_call, so a straight in-order emit replays them correctly.
pub(crate) fn messages_to_input(messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    for m in messages {
        match m.role {
            Role::System => {
                out.push(json!({ "role": "system", "content": join_text(m) }));
            }
            Role::Assistant => {
                let text = join_text(m);
                if !text.is_empty() {
                    out.push(json!({ "role": "assistant", "content": text }));
                }
                for b in &m.blocks {
                    match b {
                        ContentBlock::ProviderData { provider, data } if provider == "openai" => {
                            out.push(data.clone());
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            out.push(json!({
                                "type": "function_call",
                                "call_id": id,
                                "name": name,
                                "arguments": input.to_string(),
                            }));
                        }
                        _ => {}
                    }
                }
            }
            Role::User | Role::Tool => {
                let mut had_tool_result = false;
                for b in &m.blocks {
                    if let ContentBlock::ToolResult { id, content, .. } = b {
                        had_tool_result = true;
                        out.push(json!({
                            "type": "function_call_output",
                            "call_id": id,
                            "output": content,
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
#[path = "openai_responses_tests.rs"]
mod tests;
