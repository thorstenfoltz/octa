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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattens_tool_use_and_results() {
        let messages = vec![
            Message::user_text("how many rows?"),
            Message::assistant(vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "count_rows".into(),
                input: json!({"open_tab": "@active"}),
            }]),
            Message::tool_results(vec![ContentBlock::ToolResult {
                id: "call_1".into(),
                content: "{\"row_count\":42}".into(),
                is_error: false,
            }]),
        ];
        let wire = messages_to_wire("be helpful", &messages);
        // system, user, assistant(with tool_calls), tool.
        assert_eq!(wire[0]["role"], "system");
        assert_eq!(wire[1]["role"], "user");
        assert_eq!(wire[2]["role"], "assistant");
        assert_eq!(wire[2]["content"], Value::Null);
        assert_eq!(wire[2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(wire[2]["tool_calls"][0]["function"]["name"], "count_rows");
        assert_eq!(wire[3]["role"], "tool");
        assert_eq!(wire[3]["tool_call_id"], "call_1");
    }

    #[test]
    fn tool_schema_wraps_under_function() {
        let cfg = ProviderConfig {
            model: "gpt-4o".into(),
            base_url: None,
            api_key: "k".into(),
            temperature: 0.5,
            max_tokens: Some(100),
        };
        let tools = vec![ToolDef {
            name: "schema".into(),
            description: "desc".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        }];
        let body = build_body(
            &cfg,
            "sys",
            &[Message::user_text("hi")],
            &tools,
            "max_completion_tokens",
        );
        assert_eq!(body["stream"], true);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "schema");
        assert_eq!(body["tool_choice"], "auto");
        // The chosen token field carries the cap; the legacy field is absent.
        assert_eq!(body["max_completion_tokens"], 100);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn unlimited_tokens_omits_the_field() {
        let cfg = ProviderConfig {
            model: "gpt-4o".into(),
            base_url: None,
            api_key: "k".into(),
            temperature: 0.5,
            max_tokens: None,
        };
        let body = build_body(&cfg, "sys", &[Message::user_text("hi")], &[], "max_tokens");
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("max_completion_tokens").is_none());
    }
}
