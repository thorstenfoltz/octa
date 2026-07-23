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
        let body = build_body(cfg, system, messages, tools)?;
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

/// Turn the profile's free-text reasoning value into an Anthropic thinking
/// budget. Anthropic takes a token count, not an effort word, so `"high"` is a
/// user error worth reporting rather than silently ignoring: the alternative is
/// a profile that quietly never thinks.
///
/// Empty / absent -> `Ok(None)` (thinking off). A positive integer ->
/// `Ok(Some(n))`. Anything else -> `Err`, surfaced in the chat panel.
fn parse_thinking_budget(reasoning: Option<&str>) -> Result<Option<u32>, String> {
    let Some(s) = reasoning.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    match s.parse::<u32>() {
        Ok(n) if n > 0 => Ok(Some(n)),
        _ => Err(format!(
            "Anthropic thinking needs a token budget as a number (for example 8000), not '{s}'."
        )),
    }
}

fn build_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Result<Value, String> {
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

    // Extended thinking. Anthropic constrains the rest of the request when it
    // is on: temperature must be 1, and max_tokens must leave room for the
    // budget on top of the visible answer. Both are fixed up here so a thinking
    // profile cannot produce a request the API rejects on arrival.
    if let Some(budget) = parse_thinking_budget(cfg.reasoning.as_deref())? {
        body.insert(
            "thinking".into(),
            json!({ "type": "enabled", "budget_tokens": budget }),
        );
        body.insert("temperature".into(), json!(1.0));
        let needed = budget as usize + 1;
        let max = cfg.max_tokens.unwrap_or(16_384).max(needed);
        body.insert("max_tokens".into(), json!(max));
    }

    if !system.is_empty() {
        body.insert("system".into(), json!(system));
    }
    body.insert("messages".into(), json!(wire_messages));
    if !wire_tools.is_empty() {
        body.insert("tools".into(), json!(wire_tools));
    }
    Ok(Value::Object(body))
}

fn message_to_wire(m: &Message) -> Value {
    // Anthropic only knows "user" / "assistant"; tool results ride inside a
    // user turn.
    let role = match m.role {
        Role::Assistant => "assistant",
        _ => "user",
    };
    let content: Vec<Value> = m
        .blocks
        .iter()
        .map(block_to_wire)
        .filter(|v| !v.is_null())
        .collect();
    json!({ "role": role, "content": content })
}

fn block_to_wire(b: &ContentBlock) -> Value {
    match b {
        // Another provider's opaque payload (e.g. OpenAI reasoning items);
        // filtered out of the wire message above.
        ContentBlock::ProviderData { .. } => Value::Null,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_reasoning(reasoning: Option<&str>, max_tokens: Option<usize>) -> ProviderConfig {
        ProviderConfig {
            model: "claude-opus-4-8".into(),
            base_url: None,
            api_key: "k".into(),
            temperature: 0.0,
            max_tokens,
            reasoning: reasoning.map(str::to_string),
        }
    }

    #[test]
    fn thinking_budget_parses_a_number() {
        assert_eq!(parse_thinking_budget(Some("8000")).unwrap(), Some(8000));
    }

    #[test]
    fn blank_thinking_budget_is_none() {
        assert_eq!(parse_thinking_budget(None).unwrap(), None);
        assert_eq!(parse_thinking_budget(Some("")).unwrap(), None);
        assert_eq!(parse_thinking_budget(Some("   ")).unwrap(), None);
    }

    #[test]
    fn effort_words_are_rejected_for_anthropic() {
        // "high" is the OpenAI spelling; Anthropic wants a token count. The
        // user gets told, rather than the profile silently not thinking.
        let err = parse_thinking_budget(Some("high")).unwrap_err();
        assert!(err.contains("number"));
        assert!(parse_thinking_budget(Some("0")).is_err());
        assert!(parse_thinking_budget(Some("-5")).is_err());
    }

    #[test]
    fn no_reasoning_leaves_the_request_untouched() {
        let body = build_body(&cfg_with_reasoning(None, Some(4096)), "sys", &[], &[]).unwrap();
        assert!(body.get("thinking").is_none());
        assert_eq!(body["temperature"], json!(0.0));
        assert_eq!(body["max_tokens"], json!(4096));
    }

    #[test]
    fn thinking_forces_temperature_one_and_room_above_the_budget() {
        // Anthropic rejects thinking with temperature != 1, and requires
        // max_tokens to exceed the budget. A profile with temperature 0 and a
        // budget larger than its cap must still produce a valid request.
        let body = build_body(
            &cfg_with_reasoning(Some("8000"), Some(4096)),
            "sys",
            &[],
            &[],
        )
        .unwrap();

        assert_eq!(body["thinking"]["type"], json!("enabled"));
        assert_eq!(body["thinking"]["budget_tokens"], json!(8000));
        assert_eq!(body["temperature"], json!(1.0));
        assert!(body["max_tokens"].as_u64().unwrap() > 8000);
    }

    #[test]
    fn thinking_keeps_a_generous_max_tokens() {
        let body = build_body(
            &cfg_with_reasoning(Some("1024"), Some(32_000)),
            "sys",
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(body["max_tokens"], json!(32_000));
    }

    #[test]
    fn a_bad_reasoning_value_fails_the_request() {
        assert!(build_body(&cfg_with_reasoning(Some("high"), None), "sys", &[], &[]).is_err());
    }
}
