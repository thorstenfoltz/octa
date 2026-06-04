//! Google Gemini `generateContent` adapter (SSE via `?alt=sse`).
//!
//! Gemini has no tool-call ids: a `functionCall` carries a name + args, and
//! the matching `functionResponse` is keyed by that name. So the adapter uses
//! the function name as the neutral tool-call id, and the agent's tool result
//! round-trips back under the same name.

use std::sync::atomic::AtomicBool;

use serde_json::{Map, Value, json};

use crate::app::chat::types::{ChatEvent, ContentBlock, Message, Role, StopReason, ToolDef};

use super::{ChatProvider, ProviderConfig, stream_sse};

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct Gemini;

impl ChatProvider for Gemini {
    fn name(&self) -> &'static str {
        "gemini"
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
        let endpoint = format!("{BASE}/{}:streamGenerateContent?alt=sse", cfg.model);
        let headers = [("x-goog-api-key", cfg.api_key.clone())];
        let body = build_body(cfg, system, messages, tools);

        let mut stop_reason = StopReason::EndTurn;
        let mut saw_tool = false;
        let mut done = false;

        stream_sse(&endpoint, &headers, &body, cancel, |data| {
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
            if let Some(um) = v.get("usageMetadata") {
                sink(ChatEvent::Usage {
                    input_tokens: um["promptTokenCount"].as_u64().unwrap_or(0) as u32,
                    output_tokens: um["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
                });
            }
            let Some(cand) = v["candidates"].get(0) else {
                return Ok(false);
            };
            if let Some(parts) = cand["content"]["parts"].as_array() {
                for part in parts {
                    if let Some(text) = part["text"].as_str() {
                        if !text.is_empty() {
                            sink(ChatEvent::TextDelta(text.to_string()));
                        }
                    } else if let Some(fc) = part.get("functionCall") {
                        let name = fc["name"].as_str().unwrap_or_default().to_string();
                        let input = fc.get("args").cloned().unwrap_or_else(|| json!({}));
                        saw_tool = true;
                        sink(ChatEvent::ToolCall {
                            id: name.clone(),
                            name,
                            input,
                        });
                    }
                }
            }
            if let Some(fr) = cand["finishReason"].as_str() {
                stop_reason = match fr {
                    "STOP" => StopReason::EndTurn,
                    "MAX_TOKENS" => StopReason::MaxTokens,
                    other => StopReason::Other(other.to_string()),
                };
                sink(ChatEvent::Done {
                    stop_reason: if saw_tool {
                        StopReason::ToolUse
                    } else {
                        stop_reason.clone()
                    },
                });
                done = true;
                return Ok(true);
            }
            Ok(false)
        })?;

        // Gemini may end the stream without an explicit finishReason chunk.
        if !done {
            sink(ChatEvent::Done {
                stop_reason: if saw_tool {
                    StopReason::ToolUse
                } else {
                    stop_reason
                },
            });
        }
        Ok(())
    }
}

fn build_body(
    cfg: &ProviderConfig,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
) -> Value {
    let contents: Vec<Value> = messages.iter().map(message_to_wire).collect();

    let function_declarations: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "parameters": to_gemini_schema(t.input_schema.clone()),
            })
        })
        .collect();

    let mut body = Map::new();
    body.insert("contents".into(), json!(contents));
    if !system.is_empty() {
        body.insert(
            "systemInstruction".into(),
            json!({ "parts": [{ "text": system }] }),
        );
    }
    if !function_declarations.is_empty() {
        body.insert(
            "tools".into(),
            json!([{ "functionDeclarations": function_declarations }]),
        );
    }
    let mut generation_config = Map::new();
    generation_config.insert("temperature".into(), json!(cfg.temperature));
    // `None` => unlimited: omit the cap and let the model use its own default.
    if let Some(max) = cfg.max_tokens {
        generation_config.insert("maxOutputTokens".into(), json!(max));
    }
    body.insert("generationConfig".into(), Value::Object(generation_config));
    Value::Object(body)
}

/// Convert a `schemars` JSON Schema into Gemini's OpenAPI-subset `Schema`.
/// Gemini's `functionDeclarations[].parameters` rejects JSON-Schema keywords
/// like `$schema`, `$ref`/`$defs`, `additionalProperties`, `default`, `title`,
/// and the `anyOf` wrapper `schemars` emits for `Option`. We resolve `$ref`s
/// against the top-level `$defs`, collapse nullable wrappers into `nullable`,
/// and keep only whitelisted fields. (Fixes the `400 Unknown name "type"` /
/// invalid-payload errors.)
fn to_gemini_schema(v: Value) -> Value {
    let defs = v
        .get("$defs")
        .or_else(|| v.get("definitions"))
        .cloned()
        .unwrap_or(Value::Null);
    clean_schema(v, &defs)
}

fn is_null_schema(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("null")
}

fn clean_schema(v: Value, defs: &Value) -> Value {
    // Boolean / non-object schemas (`true`, `serde_json::Value` fields) become a
    // permissive string so Gemini still parses them.
    let mut map = match v {
        Value::Object(m) => m,
        _ => return json!({ "type": "string" }),
    };

    // Inline a `$ref` against the top-level `$defs` (current keys win).
    if let Some(Value::String(r)) = map.remove("$ref") {
        let name = r
            .strip_prefix("#/$defs/")
            .or_else(|| r.strip_prefix("#/definitions/"));
        if let Some(name) = name
            && let Some(Value::Object(target)) = defs.get(name).cloned()
        {
            let mut merged = target;
            for (k, val) in map {
                merged.insert(k, val);
            }
            return clean_schema(Value::Object(merged), defs);
        }
    }

    let mut nullable = false;

    // Collapse anyOf / oneOf / allOf: take the first non-null branch's keys and
    // mark nullable if any branch is the `null` type.
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(branches)) = map.remove(key) {
            let mut chosen: Option<Map<String, Value>> = None;
            for b in branches {
                if is_null_schema(&b) {
                    nullable = true;
                } else if let Value::Object(bm) = b
                    && chosen.is_none()
                {
                    chosen = Some(bm);
                }
            }
            if let Some(bm) = chosen {
                for (k, val) in bm {
                    map.entry(k).or_insert(val);
                }
            }
        }
    }

    let mut out = Map::new();

    match map.get("type") {
        Some(Value::String(s)) => {
            out.insert("type".into(), Value::String(s.clone()));
        }
        Some(Value::Array(arr)) => {
            let mut chosen = None;
            for item in arr {
                match item.as_str() {
                    Some("null") => nullable = true,
                    Some(s) if chosen.is_none() => chosen = Some(s.to_string()),
                    _ => {}
                }
            }
            if let Some(s) = chosen {
                out.insert("type".into(), Value::String(s));
            }
        }
        _ => {}
    }

    for k in ["description", "enum", "minItems", "maxItems", "required"] {
        if let Some(val) = map.get(k) {
            out.insert(k.to_string(), val.clone());
        }
    }

    if let Some(Value::Object(props)) = map.get("properties") {
        let cleaned: Map<String, Value> = props
            .iter()
            .map(|(n, s)| (n.clone(), clean_schema(s.clone(), defs)))
            .collect();
        out.insert("properties".into(), Value::Object(cleaned));
        out.entry("type".to_string())
            .or_insert_with(|| Value::String("object".into()));
    }
    if let Some(items) = map.get("items") {
        out.insert("items".into(), clean_schema(items.clone(), defs));
        out.entry("type".to_string())
            .or_insert_with(|| Value::String("array".into()));
    }

    if nullable {
        out.insert("nullable".into(), Value::Bool(true));
    }
    // A schema that ended up with no type at all - give Gemini a permissive
    // string so the declaration still parses.
    if !out.contains_key("type") && !out.contains_key("properties") && !out.contains_key("items") {
        out.insert("type".to_string(), Value::String("string".into()));
    }

    Value::Object(out)
}

fn message_to_wire(m: &Message) -> Value {
    let role = match m.role {
        Role::Assistant => "model",
        _ => "user",
    };
    let parts: Vec<Value> = m.blocks.iter().map(block_to_wire).collect();
    json!({ "role": role, "parts": parts })
}

fn block_to_wire(b: &ContentBlock) -> Value {
    match b {
        ContentBlock::Text { text } => json!({ "text": text }),
        ContentBlock::ToolUse { name, input, .. } => {
            json!({ "functionCall": { "name": name, "args": input } })
        }
        ContentBlock::ToolResult { id, content, .. } => {
            // `id` is the function name for Gemini. The response must be a
            // JSON object; wrap a non-object payload under `result`.
            let response = serde_json::from_str::<Value>(content)
                .ok()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({ "result": content }));
            json!({ "functionResponse": { "name": id, "response": response } })
        }
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    /// Recursively assert no key Gemini rejects survives as a *schema keyword*.
    /// Property names are arbitrary (a tool may have a param called `title` or
    /// `type`), so the values under `properties` are checked but their keys are
    /// not treated as keywords.
    fn assert_clean(v: &Value) {
        const BAD: &[&str] = &[
            "$schema",
            "$ref",
            "$defs",
            "definitions",
            "additionalProperties",
            "default",
            "title",
            "anyOf",
            "oneOf",
            "allOf",
        ];
        match v {
            Value::Object(m) => {
                for (k, val) in m {
                    if k == "properties" {
                        if let Value::Object(props) = val {
                            for pv in props.values() {
                                assert_clean(pv);
                            }
                        }
                        continue;
                    }
                    assert!(
                        !BAD.contains(&k.as_str()),
                        "leaked `{k}` into Gemini schema"
                    );
                    assert_clean(val);
                }
            }
            Value::Array(a) => {
                for val in a {
                    assert_clean(val);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn nullable_option_collapses() {
        let s = json!({
            "type": "object",
            "properties": {
                "open_tab": { "anyOf": [ { "type": "string" }, { "type": "null" } ] }
            }
        });
        let out = to_gemini_schema(s);
        let ot = &out["properties"]["open_tab"];
        assert_eq!(ot["type"], "string");
        assert_eq!(ot["nullable"], true);
        assert_clean(&out);
    }

    #[test]
    fn ref_is_inlined_and_defs_dropped() {
        let s = json!({
            "type": "object",
            "properties": { "spec": { "$ref": "#/$defs/Spec" } },
            "$defs": { "Spec": { "type": "object", "properties": { "n": { "type": "integer" } } } }
        });
        let out = to_gemini_schema(s);
        assert_eq!(out["properties"]["spec"]["type"], "object");
        assert_eq!(
            out["properties"]["spec"]["properties"]["n"]["type"],
            "integer"
        );
        assert_clean(&out);
    }

    #[test]
    fn every_tool_schema_is_gemini_clean() {
        for def in crate::app::chat::tools::tool_defs() {
            let cleaned = to_gemini_schema(def.input_schema.clone());
            assert_eq!(cleaned["type"], "object", "tool {}", def.name);
            assert_clean(&cleaned);
        }
    }
}
