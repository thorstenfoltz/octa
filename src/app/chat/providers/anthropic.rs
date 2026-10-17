//! Anthropic Messages API adapter (Claude).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, Reasoning, parse_reasoning, stream_sse};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const MODELS_ENDPOINT: &str = "https://api.anthropic.com/v1/models";
const API_VERSION: &str = "2023-06-01";

/// What "unlimited" falls back to when the Models API cannot be reached. Low
/// enough that every Claude model ever released accepts it, since the point of
/// the fallback is to still produce a *valid* request when the lookup failed.
const FALLBACK_MAX_TOKENS: usize = 16_384;

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
        let cfg = resolve_max_tokens(cfg);
        let cfg = cfg.as_ref();
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

/// Per-model `max_tokens` ceilings, learned from the Models API and kept for
/// the process lifetime. A model's ceiling does not change under a running
/// program, so one lookup each is enough.
static MODEL_CAPS: RwLock<BTreeMap<String, usize>> = RwLock::new(BTreeMap::new());

/// Turn an "unlimited" profile into the largest number this model actually
/// takes.
///
/// Anthropic is the one provider that *requires* `max_tokens`, so "unlimited"
/// cannot be expressed by omitting the field the way it is everywhere else. It
/// used to become a flat 16,384, which stopped meaning "unlimited" a long time
/// ago: the Claude 5 generation writes up to 128k output tokens, so the setting
/// silently cost the user 87% of the answer, and a long tool-driven reply was
/// truncated mid-sentence for no reason the UI could explain.
///
/// The ceiling is asked of the API rather than kept in a table here, because a
/// table is wrong the day a model ships: `GET /v1/models/{id}` reports
/// `max_tokens` per model (128k on Opus 5 and Fable 5.1, 64k on Haiku 4.5,
/// less on older ones), so a model released after this build still gets its
/// real ceiling. A failed lookup falls back to the old constant, which every
/// model accepts, so the worst case is the behaviour we had before.
fn resolve_max_tokens(cfg: &ProviderConfig) -> Cow<'_, ProviderConfig> {
    if cfg.max_tokens.is_some() {
        return Cow::Borrowed(cfg);
    }
    let cap = model_max_tokens(&cfg.model, &cfg.api_key).unwrap_or(FALLBACK_MAX_TOKENS);
    Cow::Owned(ProviderConfig {
        max_tokens: Some(cap),
        ..cfg.clone()
    })
}

/// This model's `max_tokens` ceiling, from cache or from the Models API. One
/// small GET, once per model per process, and only for a profile that asked
/// for unlimited. Every failure (no key, offline, unknown model, a body that
/// does not parse) is `None`: the caller falls back rather than failing a turn
/// over a lookup the user did not ask for.
fn model_max_tokens(model: &str, api_key: &str) -> Option<usize> {
    if let Ok(cache) = MODEL_CAPS.read()
        && let Some(cap) = cache.get(model)
    {
        return Some(*cap);
    }
    let body = ureq::get(format!("{MODELS_ENDPOINT}/{model}"))
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let cap = parse_model_max_tokens(&body)?;
    if let Ok(mut cache) = MODEL_CAPS.write() {
        cache.insert(model.to_string(), cap);
    }
    Some(cap)
}

/// Pull `max_tokens` out of a Models API response. Null, zero and a missing
/// field all mean "the API did not tell us", which is not the same as zero.
fn parse_model_max_tokens(body: &str) -> Option<usize> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("max_tokens")?
        .as_u64()
        .filter(|n| *n > 0)
        .map(|n| n as usize)
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
    // Anthropic requires `max_tokens`. An "unlimited" profile was already
    // turned into this model's real ceiling by `resolve_max_tokens`, so the
    // fallback here only covers a caller that skipped that step.
    body.insert(
        "max_tokens".into(),
        json!(cfg.max_tokens.unwrap_or(FALLBACK_MAX_TOKENS)),
    );
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
            let max = cfg.max_tokens.unwrap_or(FALLBACK_MAX_TOKENS).max(needed);
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
#[path = "anthropic_tests.rs"]
mod tests;
