//! Provider abstraction: one trait, one file per provider. Each adapter
//! translates the neutral [`Message`]/[`ToolDef`] model to its wire format,
//! POSTs over the blocking `ureq` client, and parses the SSE stream back into
//! [`ChatEvent`]s. The agent worker thread owns the blocking call; the GUI
//! never touches the network.

pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod openai_compat;

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::ui::settings::ChatProviderKind;

use super::types::{ChatEvent, Message, ToolDef};

/// Everything a provider needs for one turn that isn't the conversation.
#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub model: String,
    /// Base URL for the OpenAI-compatible provider; ignored by the others.
    pub base_url: Option<String>,
    pub api_key: String,
    pub temperature: f32,
    /// Response-token cap. `None` means "unlimited": providers omit the field
    /// (Anthropic, which requires it, substitutes a high default instead).
    pub max_tokens: Option<usize>,
    /// Free-text thinking/reasoning value from the profile; `None`/empty omits
    /// it entirely. Each provider maps it to its own knob: OpenAI (and the
    /// compatible/Ollama endpoints) to `reasoning_effort`, Anthropic to a
    /// numeric `thinking.budget_tokens`, Gemini to `thinkingConfig`. A value a
    /// provider cannot use surfaces as an error rather than being silently
    /// dropped.
    pub reasoning: Option<String>,
}

/// A chat backend. `stream_turn` blocks until the turn finishes or `cancel`
/// flips, pushing every [`ChatEvent`] through `sink` as it arrives.
pub trait ChatProvider: Send {
    fn name(&self) -> &'static str;

    fn stream_turn(
        &self,
        cfg: &ProviderConfig,
        system: &str,
        messages: &[Message],
        tools: &[ToolDef],
        cancel: &AtomicBool,
        sink: &mut dyn FnMut(ChatEvent),
    ) -> Result<(), String>;
}

/// Construct the provider adapter for a settings enum value.
pub fn make_provider(kind: ChatProviderKind) -> Box<dyn ChatProvider> {
    match kind {
        ChatProviderKind::Anthropic => Box::new(anthropic::Anthropic),
        ChatProviderKind::OpenAi => Box::new(openai::OpenAi),
        ChatProviderKind::OpenAiCompatible => Box::new(openai_compat::OpenAiCompat),
        ChatProviderKind::Gemini => Box::new(gemini::Gemini),
        ChatProviderKind::Ollama => Box::new(ollama::Ollama),
    }
}

/// POST `body` to `url` with `headers`, then stream the response as
/// Server-Sent Events, handing each `data:` payload to `on_data`. The reader
/// is unbuffered at the body level (`into_reader`) so chunks surface live -
/// `read_to_string` / `.limit()` would block until the whole body arrived and
/// defeat streaming. `on_data` returns `Ok(true)` to stop early (e.g. on
/// `[DONE]`). `cancel` is polled between lines.
pub(crate) fn stream_sse(
    url: &str,
    headers: &[(&str, String)],
    body: &Value,
    cancel: &AtomicBool,
    mut on_data: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    // Configure on an Agent rather than per-request: a request-level
    // `.config()...build()` erases ureq's `WithBody` type-state and drops
    // `send_json`. `http_status_as_error(false)` surfaces non-2xx as a normal
    // response so we can read the error body the provider returned.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.post(url).header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }

    let resp = req
        .send_json(body)
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp
            .into_body()
            .read_to_string()
            .unwrap_or_else(|_| "<no body>".to_string());
        // Most providers (OpenAI / Ollama / Gemini / Anthropic) return a JSON
        // body shaped `{"error":{"message":...}}` or `{"message":...}`; surface
        // just that message rather than the raw JSON.
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| text.trim().to_string());
        return Err(format!("HTTP {}: {detail}", status.as_u16()));
    }

    let reader = resp.into_body().into_reader();
    let mut buf = BufReader::new(reader);
    let mut line = String::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        line.clear();
        let n = buf
            .read_line(&mut line)
            .map_err(|e| format!("stream read failed: {e}"))?;
        if n == 0 {
            break; // EOF
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        // SSE field lines: `data: <payload>`. Ignore `event:` / `id:` /
        // comments and blank separators - the payload JSON carries its own
        // type tag for every provider we target.
        let Some(payload) = trimmed.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim_start();
        if payload.is_empty() {
            continue;
        }
        if on_data(payload)? {
            break;
        }
    }
    Ok(())
}
