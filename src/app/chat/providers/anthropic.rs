//! Anthropic Messages API adapter (Claude).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, stream_sse};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct Anthropic;

impl ChatProvider for Anthropic {
    fn name(&self) -> &'static str {
        "anthropic"
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
        let body = build_body(cfg, system, messages, tools);
        let headers = [
            ("x-api-key", cfg.api_key.clone()),
            ("anthropic-version", API_VERSION.to_string()),
        ];

        // Per-content-block accumulation for tool_use args.
        let mut blocks: HashMap<i64, ToolAccum> = HashMap::new();
        let mut stop_reason = StopReason::EndTurn;

        stream_sse(ENDPOINT, &headers, &body, cancel, |data| {
            let v: Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => return Ok(false),
            };
            match v.get("type").and_then(Value::as_str).unwrap_or("") {
                "content_block_start" => {
                    let idx = v["index"].as_i64().unwrap_or(0);
                    let cb = &v["content_block"];
                    if cb.get("type").and_then(Value::as_str) == Some("tool_use") {
                        blocks.insert(
                            idx,
                            ToolAccum {
                                id: cb["id"].as_str().unwrap_or_default().to_string(),
                                name: cb["name"].as_str().unwrap_or_default().to_string(),
                                args: String::new(),
                            },
                        );
                    }
                }
                "content_block_delta" => {
                    let idx = v["index"].as_i64().unwrap_or(0);
                    let delta = &v["delta"];
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => {
                            if let Some(t) = delta["text"].as_str() {
                                sink(ChatEvent::TextDelta(t.to_string()));
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(acc) = blocks.get_mut(&idx)
                                && let Some(p) = delta["partial_json"].as_str()
                            {
                                acc.args.push_str(p);
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    let idx = v["index"].as_i64().unwrap_or(0);
                    if let Some(acc) = blocks.remove(&idx) {
                        let input = if acc.args.trim().is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(&acc.args).unwrap_or(json!({}))
                        };
                        sink(ChatEvent::ToolCall {
                            id: acc.id,
                            name: acc.name,
                            input,
                        });
                    }
                }
                "message_delta" => {
                    if let Some(sr) = v["delta"]["stop_reason"].as_str() {
                        stop_reason = map_stop_reason(sr);
                    }
                    if let Some(out) = v["usage"]["output_tokens"].as_u64() {
                        sink(ChatEvent::Usage {
                            input_tokens: 0,
                            output_tokens: out as u32,
                        });
                    }
                }
                "message_stop" => {
                    sink(ChatEvent::Done {
                        stop_reason: stop_reason.clone(),
                    });
                    return Ok(true);
                }
                "error" => {
                    let msg = v["error"]["message"]
                        .as_str()
                        .unwrap_or("unknown error")
                        .to_string();
                    sink(ChatEvent::Error(msg));
                    return Ok(true);
                }
                _ => {}
            }
            Ok(false)
        })
    }
}

struct ToolAccum {
    id: String,
    name: String,
    args: String,
}

fn map_stop_reason(s: &str) -> StopReason {
    match s {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        other => StopReason::Other(other.to_string()),
    }
}

fn build_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Value {
    let wire_messages: Vec<Value> = messages.iter().map(message_to_wire).collect();
    let wire_tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        })
        .collect();

    let mut body = Map::new();
    body.insert("model".into(), json!(cfg.model));
    // Anthropic requires `max_tokens`; an "unlimited" choice maps to a high
    // ceiling rather than omitting the field.
    body.insert("max_tokens".into(), json!(cfg.max_tokens.unwrap_or(16_384)));
    body.insert("temperature".into(), json!(cfg.temperature));
    body.insert("stream".into(), json!(true));
    if !system.is_empty() {
        body.insert("system".into(), json!(system));
    }
    body.insert("messages".into(), json!(wire_messages));
    if !wire_tools.is_empty() {
        body.insert("tools".into(), json!(wire_tools));
    }
    Value::Object(body)
}

fn message_to_wire(m: &Message) -> Value {
    // Anthropic only knows "user" / "assistant"; tool results ride inside a
    // user turn.
    let role = match m.role {
        Role::Assistant => "assistant",
        _ => "user",
    };
    let content: Vec<Value> = m.blocks.iter().map(block_to_wire).collect();
    json!({ "role": role, "content": content })
}

fn block_to_wire(b: &ContentBlock) -> Value {
    match b {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::ToolUse { id, name, input } => {
            json!({ "type": "tool_use", "id": id, "name": name, "input": input })
        }
        ContentBlock::ToolResult {
            id,
            content,
            is_error,
        } => json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": content,
            "is_error": is_error,
        }),
    }
}
