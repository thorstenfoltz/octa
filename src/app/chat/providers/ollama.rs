//! Local Ollama adapter. Ollama exposes an OpenAI-compatible API at
//! `/v1/chat/completions`, so the streaming + message conversion is reused
//! verbatim from `openai.rs`; only the endpoint differs and no auth header is
//! sent. Server lifecycle + model discovery live in `crate::app::chat::ollama`.

use std::sync::atomic::AtomicBool;

use crate::app::chat::types::{ChatEvent, Message, ToolDef};

use super::openai::run_openai;
use super::{ChatProvider, ProviderConfig};

/// Default root URL when none is configured.
pub const DEFAULT_URL: &str = "http://localhost:11434";

pub struct Ollama;

impl ChatProvider for Ollama {
    fn name(&self) -> &'static str {
        "ollama"
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
            .unwrap_or(DEFAULT_URL)
            .trim_end_matches('/');
        let endpoint = format!("{base}/v1/chat/completions");
        // Local Ollama needs no Authorization header.
        let headers: [(&str, String); 0] = [];
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
