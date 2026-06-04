//! OpenAI-compatible adapter: same wire dialect as `openai.rs`, but against a
//! user-supplied base URL (Ollama, OpenRouter, Groq, LM Studio, ...). Reuses
//! the OpenAI streaming driver and message conversion verbatim.

use std::sync::atomic::AtomicBool;

use crate::app::chat::types::{ChatEvent, Message, ToolDef};

use super::openai::run_openai;
use super::{ChatProvider, ProviderConfig};

pub struct OpenAiCompat;

impl ChatProvider for OpenAiCompat {
    fn name(&self) -> &'static str {
        "openai_compat"
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
        let base = cfg
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "no base URL set for the OpenAI-compatible provider - set it in chat settings"
                    .to_string()
            })?;
        let endpoint = join_url(base, "chat/completions");
        // Many local servers (Ollama, LM Studio) ignore the key; send it
        // anyway so hosted compatible gateways (OpenRouter, Groq) authenticate.
        let headers = [("authorization", format!("Bearer {}", cfg.api_key))];
        run_openai(
            &endpoint,
            &headers,
            cfg,
            system,
            messages,
            tools,
            "max_tokens",
            cancel,
            sink,
        )
    }
}

/// Join a base URL and a path, tolerating a trailing slash and an already
/// `/v1`-suffixed base.
fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/{path}")
}
