//! OpenAI Chat Completions adapter. The wire helpers here are reused by the
//! OpenAI-compatible provider (`openai_compat.rs`) against a different base
//! URL.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, stream_sse};

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAi;

impl ChatProvider for OpenAi {
    fn name(&self) -> &'static str {
        "openai"
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
        let headers = [("authorization", format!("Bearer {}", cfg.api_key))];
        // Newer OpenAI models reject the legacy `max_tokens` field and require
        // `max_completion_tokens`.
        run_openai(
            ENDPOINT,
            &headers,
            cfg,
            system,
            messages,
            tools,
            "max_completion_tokens",
            cancel,
            sink,
        )
    }
}

/// Shared streaming driver for OpenAI and OpenAI-compatible endpoints.
/// `token_field` is the request key carrying the response-token cap:
/// `max_completion_tokens` for OpenAI proper, `max_tokens` for the broadly
/// compatible local servers (Ollama, LM Studio, OpenRouter, ...).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_openai(
    endpoint: &str,
    headers: &[(&str, String)],
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    token_field: &str,
    cancel: &AtomicBool,
    sink: &mut dyn FnMut(ChatEvent),
) -> Result<(), String> {
    let body = build_body(cfg, system, messages, tools, token_field);

    // index -> (id, name, accumulated-args). Ordered so the final emit is
    // deterministic.
    let mut calls: BTreeMap<i64, ToolAccum> = BTreeMap::new();
    let mut stop_reason = StopReason::EndTurn;
    let mut done = false;

    stream_sse(endpoint, headers, &body, cancel, |data| {
        if data == "[DONE]" {
            flush(&mut calls, sink);
            if !done {
                sink(ChatEvent::Done {
                    stop_reason: stop_reason.clone(),
                });
            }
            return Ok(true);
        }
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        if let Some(err) = v.get("error") {
            let msg = err["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            sink(ChatEvent::Error(msg));
            return Ok(true);
        }
        if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
            sink(ChatEvent::Usage {
                input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
            });
        }
        let Some(choice) = v["choices"].get(0) else {
            return Ok(false);
        };
        let delta = &choice["delta"];
        if let Some(text) = delta["content"].as_str()
            && !text.is_empty()
        {
            sink(ChatEvent::TextDelta(text.to_string()));
        }
        if let Some(tcs) = delta["tool_calls"].as_array() {
            for tc in tcs {
                let idx = tc["index"].as_i64().unwrap_or(0);
                let acc = calls.entry(idx).or_default();
                if let Some(id) = tc["id"].as_str()
                    && !id.is_empty()
                {
                    acc.id = id.to_string();
                }
                if let Some(name) = tc["function"]["name"].as_str()
                    && !name.is_empty()
                {
                    acc.name = name.to_string();
                }
                if let Some(args) = tc["function"]["arguments"].as_str() {
                    acc.args.push_str(args);
                }
            }
        }
        if let Some(fr) = choice["finish_reason"].as_str() {
            stop_reason = match fr {
                "stop" => StopReason::EndTurn,
                "length" => StopReason::MaxTokens,
                "tool_calls" | "function_call" => StopReason::ToolUse,
                other => StopReason::Other(other.to_string()),
            };
            flush(&mut calls, sink);
            sink(ChatEvent::Done {
                stop_reason: stop_reason.clone(),
            });
            done = true;
            return Ok(true);
        }
        Ok(false)
    })
}

#[derive(Default)]
struct ToolAccum {
    id: String,
    name: String,
    args: String,
}

/// Emit every accumulated tool call, parsing its argument JSON.
fn flush(calls: &mut BTreeMap<i64, ToolAccum>, sink: &mut dyn FnMut(ChatEvent)) {
    for (_, acc) in std::mem::take(calls) {
        if acc.name.is_empty() {
            continue;
        }
        let input = if acc.args.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&acc.args).unwrap_or(json!({}))
        };
        sink(ChatEvent::ToolCall {
            id: if acc.id.is_empty() {
                acc.name.clone()
            } else {
                acc.id
            },
            name: acc.name,
            input,
        });
    }
}

pub(crate) fn build_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    token_field: &str,
) -> Value {
    let wire_messages = messages_to_wire(system, messages);
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

    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    body.insert("messages".into(), json!(wire_messages));
    body.insert("temperature".into(), json!(cfg.temperature));
    // The profile's thinking value maps to `reasoning_effort` (low/medium/high
    // on current models). Passed through verbatim: an unsupported value is the
    // API's to reject, and hard-coding the accepted set here would go stale.
    // Blank means "no thinking", so the field is omitted rather than sent empty.
    if let Some(effort) = cfg
        .reasoning
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        body.insert("reasoning_effort".into(), json!(effort));
    }
    // `None` => unlimited: omit the cap entirely so the model uses its default.
    if let Some(max) = cfg.max_tokens {
        body.insert(token_field.into(), json!(max));
    }
    body.insert("stream".into(), json!(true));
    // Ask for usage in the final streamed chunk where supported.
    body.insert("stream_options".into(), json!({ "include_usage": true }));
    if !wire_tools.is_empty() {
        body.insert("tools".into(), json!(wire_tools));
        body.insert("tool_choice".into(), json!("auto"));
    }
    Value::Object(body)
}

/// Flatten the neutral transcript into OpenAI's message list. Assistant
/// tool-use blocks become `tool_calls`; tool results become separate
/// `{role:"tool"}` messages.
pub(crate) fn messages_to_wire(system: &str, messages: &[Message]) -> Vec<Value> {
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
                let text = join_text(m);
                let tool_calls: Vec<Value> = m
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": input.to_string(),
                            }
                        })),
                        _ => None,
                    })
                    .collect();
                let mut msg = Map::new();
                msg.insert("role".into(), json!("assistant"));
                // content must be present; null is allowed when tool_calls set.
                if text.is_empty() && !tool_calls.is_empty() {
                    msg.insert("content".into(), Value::Null);
                } else {
                    msg.insert("content".into(), json!(text));
                }
                if !tool_calls.is_empty() {
                    msg.insert("tool_calls".into(), json!(tool_calls));
                }
                out.push(Value::Object(msg));
            }
            Role::User | Role::Tool => {
                // Tool results become their own `tool` messages; remaining text
                // becomes a `user` message.
                let mut had_tool_result = false;
                for b in &m.blocks {
                    if let ContentBlock::ToolResult { id, content, .. } = b {
                        had_tool_result = true;
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
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
#[path = "openai_tests.rs"]
mod tests;
