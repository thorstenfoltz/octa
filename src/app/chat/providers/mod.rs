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
pub mod openai_responses;

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
    /// Sampling temperature, or `None` to omit the field from the request.
    /// Newer models reject `temperature` outright (Anthropic's Opus 4.7
    /// generation answers a 400), so every provider must be able to send
    /// nothing at all rather than a default.
    pub temperature: Option<f32>,
    /// Response-token cap. `None` means "unlimited": providers omit the field.
    /// Anthropic requires it, so there it becomes the model's own ceiling,
    /// looked up from the Models API (see `anthropic::resolve_max_tokens`).
    pub max_tokens: Option<usize>,
    /// Free-text thinking/reasoning value from the profile; `None`/empty omits
    /// it entirely. Each provider maps it to its own knob: OpenAI (and the
    /// compatible/Ollama endpoints) to `reasoning_effort`, Anthropic to a
    /// numeric `thinking.budget_tokens`, Gemini to `thinkingConfig`. A value a
    /// provider cannot use surfaces as an error rather than being silently
    /// dropped.
    pub reasoning: Option<String>,
    /// How wordy the visible answer should be (`low` / `medium` / `high`),
    /// or `None` to omit the field. OpenAI's `text.verbosity`, and separate
    /// from `reasoning`: effort buys thinking, verbosity buys prose, and a
    /// model can think hard and answer in one line. Ignored by the providers
    /// that have no such control.
    pub verbosity: Option<String>,
    /// Ask OpenAI for `reasoning.mode: "pro"`, which runs the model's slower,
    /// more thorough path at the same per-token price. GPT-5.6 only; other
    /// models answer 400, which is why it is off unless the profile asks.
    pub pro_mode: bool,
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

/// What the profile's free-text thinking value turned out to be. Every
/// provider now has **two** knobs and picks by shape: an effort *word* is the
/// modern one (Anthropic `output_config.effort`, OpenAI `reasoning_effort`,
/// Gemini `thinkingConfig.thinkingLevel`), a token *number* the older one
/// (Anthropic `budget_tokens`, Gemini `thinkingBudget`). Current Claude models
/// answer 400 to a budget and current Gemini models prefer the level, so the
/// word is what a user normally wants; the number stays for the older models
/// that only understand it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Reasoning {
    /// An effort word, passed through verbatim so new levels keep working.
    Effort(String),
    /// A token budget. Range checking is the provider's, not ours: Anthropic
    /// wants >= 1024, Gemini gives 0 and -1 their own meanings.
    Budget(i64),
}

/// Classify the profile's thinking value. Blank / absent means thinking off.
/// Anything that parses as an integer is a budget, anything else an effort
/// word: no fixed vocabulary, so a level a provider adds tomorrow works today.
pub(crate) fn parse_reasoning(raw: Option<&str>) -> Option<Reasoning> {
    let s = raw.map(str::trim).filter(|s| !s.is_empty())?;
    Some(match s.parse::<i64>() {
        Ok(n) => Reasoning::Budget(n),
        Err(_) => Reasoning::Effort(s.to_string()),
    })
}

/// Build the per-turn config a profile describes. Shared by the chat panel's
/// real turns and the Settings "Test connection" button, so a passing test
/// means the exact request shape the assistant will send is accepted.
///
/// `fallback_base_url` is the global Ollama / OpenAI-compatible URL, used only
/// when the profile carries none of its own (an existing setup keeps working
/// after the profile migration without re-entering the URL).
pub fn config_for_profile(
    profile: &crate::ui::settings::chat_profiles::ChatModelProfile,
    fallback_base_url: &str,
    api_key: String,
    max_tokens: Option<usize>,
) -> ProviderConfig {
    let model = if profile.model.trim().is_empty() {
        crate::ui::settings::chat_models::default_model(profile.kind)
    } else {
        profile.model.clone()
    };
    ProviderConfig {
        model,
        base_url: match profile.kind {
            ChatProviderKind::OpenAiCompatible | ChatProviderKind::Ollama => {
                let own = profile.base_url.trim();
                Some(if own.is_empty() {
                    fallback_base_url.to_string()
                } else {
                    own.to_string()
                })
            }
            _ => None,
        },
        api_key,
        temperature: profile.temperature,
        max_tokens,
        reasoning: {
            let r = profile.reasoning.trim();
            (!r.is_empty()).then(|| r.to_string())
        },
        verbosity: {
            let v = profile.verbosity.trim();
            (!v.is_empty()).then(|| v.to_string())
        },
        pro_mode: profile.pro_mode,
    }
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
    // Two bounded waits, and deliberately no global one: a long answer may
    // legitimately stream for minutes, but a server that never accepts the
    // connection, or accepts and then says nothing, must not wedge the worker
    // forever - which is what blocked the Ask boxes for a whole session, since
    // their cancel flag is not reachable from any UI.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(120)))
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
