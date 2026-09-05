//! Anthropic Messages API adapter (Claude).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, Reasoning, parse_reasoning, stream_sse};

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
                // The only place Anthropic reports the prompt size. Cached
                // tokens are counted too: `input_tokens` excludes whatever the
                // cache served, and the meter means "tokens this turn sent",
                // not "tokens we were charged full price for".
                "message_start" => {
                    let u = &v["message"]["usage"];
                    let field = |k: &str| u[k].as_u64().unwrap_or(0);
                    let total = field("input_tokens")
                        + field("cache_creation_input_tokens")
                        + field("cache_read_input_tokens");
                    if total > 0 {
                        sink(ChatEvent::Usage {
                            input_tokens: total as u32,
                            output_tokens: 0,
                        });
                    }
                }
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

/// The smallest `budget_tokens` the Messages API accepts.
const MIN_THINKING_BUDGET: i64 = 1024;

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
    if let Some(t) = cfg.temperature {
        body.insert("temperature".into(), json!(t));
    }
    body.insert("stream".into(), json!(true));

    // Anthropic has two thinking controls and the model decides which one is
    // legal. **Adaptive thinking** (an effort word in `output_config`) is the
    // current one; every model from Claude Opus 4.7 on answers 400 to the old
    // `thinking.budget_tokens`. **Extended thinking** (a token budget) is the
    // only one Claude 4.5 and earlier understand, including Haiku 4.5. So the
    // profile's value picks the shape, and the user picks the value.
    match parse_reasoning(cfg.reasoning.as_deref()) {
        None => {}
        // Passed through verbatim (low / medium / high / xhigh / max today),
        // so a level Anthropic adds later needs no release here.
        Some(Reasoning::Effort(level)) => {
            body.insert("output_config".into(), json!({ "effort": level }));
        }
        // Extended thinking constrains the rest of the request: a temperature
        // that is sent must be 1, and max_tokens must leave room for the budget
        // on top of the visible answer. Both are fixed up here so a thinking
        // profile cannot produce a request the API rejects on arrival. A
        // profile that sends no temperature keeps sending none; 1.0 is the API
        // default anyway, and the newer models reject the field outright.
        Some(Reasoning::Budget(budget)) => {
            if budget < MIN_THINKING_BUDGET {
                return Err(format!(
                    "Anthropic thinking: a token budget must be at least \
                     {MIN_THINKING_BUDGET}, and only Claude 4.5 and older models take one \
                     at all. For a current model use an effort word instead \
                     (low / medium / high / xhigh / max)."
                ));
            }
            body.insert(
                "thinking".into(),
                json!({ "type": "enabled", "budget_tokens": budget }),
            );
            if cfg.temperature.is_some() {
                body.insert("temperature".into(), json!(1.0));
            }
            let needed = budget as usize + 1;
            let max = cfg.max_tokens.unwrap_or(16_384).max(needed);
            body.insert("max_tokens".into(), json!(max));
        }
    }

    // Prompt caching. Anthropic hashes the request prefix in a fixed order -
    // tools, then system, then messages - and a `cache_control` marker says
    // "cache everything up to here". One marker on the system block therefore
    // covers the whole tool payload as well, which is the part that repeats
    // unchanged on every request of every turn and dwarfs everything else.
    //
    // A second marker on the last message caches the conversation so far, so a
    // long chat stops re-reading its own history at full price. Two markers is
    // well inside Anthropic's limit of four.
    //
    // Nothing here changes what the model sees, and an entry expiring (five
    // minutes idle) costs a normal uncached request, so there is no failure
    // mode to handle.
    let mut wire_messages = wire_messages;
    if !system.is_empty() {
        body.insert(
            "system".into(),
            json!([{
                "type": "text",
                "text": system,
                "cache_control": { "type": "ephemeral" },
            }]),
        );
    }
    if let Some(last) = wire_messages.last_mut()
        && let Some(block) = last["content"].as_array_mut().and_then(|b| b.last_mut())
        && let Some(obj) = block.as_object_mut()
    {
        obj.insert("cache_control".into(), json!({ "type": "ephemeral" }));
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
            temperature: Some(0.0),
            max_tokens,
            reasoning: reasoning.map(str::to_string),
        }
    }

    #[test]
    fn an_effort_word_becomes_adaptive_thinking() {
        // Claude Opus 4.7 and later reject `thinking.budget_tokens` outright;
        // effort in output_config is the control they do accept. A word must
        // therefore produce output_config and NOT a thinking block, and must
        // leave temperature and max_tokens alone.
        for level in ["low", "medium", "high", "xhigh", "max"] {
            let body = build_body(
                &cfg_with_reasoning(Some(level), Some(4096)),
                "sys",
                &[],
                &[],
            )
            .unwrap();
            assert_eq!(body["output_config"]["effort"], json!(level));
            assert!(body.get("thinking").is_none());
            assert_eq!(body["temperature"], json!(0.0));
            assert_eq!(body["max_tokens"], json!(4096));
        }
    }

    #[test]
    fn an_unknown_effort_word_is_still_sent() {
        // The accepted set changes with every model; a level we have never
        // heard of must reach the API rather than being refused locally.
        let body = build_body(&cfg_with_reasoning(Some("ultra"), None), "sys", &[], &[]).unwrap();
        assert_eq!(body["output_config"]["effort"], json!("ultra"));
    }

    #[test]
    fn a_budget_below_the_minimum_is_refused_locally() {
        // 1024 is the documented floor; 0 and negatives were already invalid.
        for bad in ["0", "-5", "512"] {
            let err =
                build_body(&cfg_with_reasoning(Some(bad), None), "sys", &[], &[]).unwrap_err();
            assert!(err.contains("1024"), "{bad}: {err}");
            // and it points at the control current models actually take
            assert!(err.contains("effort"), "{bad}: {err}");
        }
    }

    #[test]
    fn no_reasoning_leaves_the_request_untouched() {
        let body = build_body(&cfg_with_reasoning(None, Some(4096)), "sys", &[], &[]).unwrap();
        assert!(body.get("thinking").is_none());
        assert!(body.get("output_config").is_none());
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
    fn no_temperature_means_no_temperature_field() {
        // Claude Opus 4.7 and newer answer 400 when `temperature` is present at
        // all, so a profile with the field cleared must send a body without it -
        // including the thinking path, which otherwise pins it to 1.
        let mut cfg = cfg_with_reasoning(None, Some(4096));
        cfg.temperature = None;
        let body = build_body(&cfg, "sys", &[], &[]).unwrap();
        assert!(body.get("temperature").is_none());

        cfg.reasoning = Some("8000".into());
        let body = build_body(&cfg, "sys", &[], &[]).unwrap();
        assert!(body.get("temperature").is_none());
        assert_eq!(body["thinking"]["budget_tokens"], json!(8000));
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
    fn blank_reasoning_is_thinking_off() {
        for blank in [None, Some(""), Some("   ")] {
            let body = build_body(&cfg_with_reasoning(blank, None), "sys", &[], &[]).unwrap();
            assert!(body.get("thinking").is_none());
            assert!(body.get("output_config").is_none());
        }
    }

    /// The cache marker sits on the system block, which in Anthropic's fixed
    /// prefix order (tools, system, messages) also covers the tool payload -
    /// the part that repeats unchanged on every request and dwarfs the rest.
    #[test]
    fn the_system_block_carries_the_cache_marker() {
        let tools = [ToolDef {
            name: "read_table".into(),
            description: "read it".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }];
        let body = build_body(&cfg_with_reasoning(None, None), "sys", &[], &tools).unwrap();
        assert_eq!(body["system"][0]["text"], json!("sys"));
        assert_eq!(
            body["system"][0]["cache_control"]["type"],
            json!("ephemeral")
        );
        // The tools ride in front of it, uncached in their own right.
        assert_eq!(body["tools"][0]["name"], json!("read_table"));
        assert!(body["tools"][0].get("cache_control").is_none());
    }

    /// The second marker rides the last message, so a long conversation stops
    /// re-reading its own history at full price. Only the last one gets it.
    #[test]
    fn the_last_message_carries_the_rolling_marker() {
        let msgs = [
            Message {
                role: Role::User,
                blocks: vec![ContentBlock::Text {
                    text: "first".into(),
                }],
            },
            Message {
                role: Role::User,
                blocks: vec![ContentBlock::Text {
                    text: "second".into(),
                }],
            },
        ];
        let body = build_body(&cfg_with_reasoning(None, None), "sys", &msgs, &[]).unwrap();
        assert!(
            body["messages"][0]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            body["messages"][1]["content"][0]["cache_control"]["type"],
            json!("ephemeral")
        );
    }

    /// No system prompt means no system field at all, marker or not.
    #[test]
    fn an_empty_system_prompt_sends_no_system_field() {
        let body = build_body(&cfg_with_reasoning(None, None), "", &[], &[]).unwrap();
        assert!(body.get("system").is_none());
    }
}
