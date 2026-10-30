//! Named chat model profiles.
//!
//! A profile is one complete "who am I talking to" configuration: a provider,
//! a model, a temperature, an optional thinking/reasoning value, and a name the
//! user chose. There can be arbitrarily many, including several for the same
//! provider (an Anthropic "Opus, deep thinking" beside an Anthropic "Sonnet,
//! quick"), so the assistant panel switches between them with one dropdown
//! instead of the user re-editing settings each time.
//!
//! This supersedes the old one-config-per-provider chat settings
//! (`chat_provider` + `chat_models[kind]` + `chat_temperature`), which are kept
//! as the migration source and as fallbacks: [`ensure_profiles`] folds them
//! into a single seed profile the first time it runs, so an existing install
//! comes up with exactly the setup it had before.
//!
//! API keys stay shared per provider by default. A profile can opt into its own
//! key with `use_own_key`, in which case the key is stored under the profile's
//! `id` (see `secrets::get_key_for`).

use serde::{Deserialize, Serialize};

use super::{AppSettings, ChatProviderKind, chat_models};

/// One named chat configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatModelProfile {
    /// Stable slug id, also used as the keyring entry name for a per-profile
    /// key. Derived from the name once, at creation, and never changed after:
    /// renaming a profile must not orphan its stored key.
    pub id: String,
    /// User-facing name shown in the panel dropdown.
    pub name: String,
    /// Optional free-text note ("cheap, for bulk work").
    #[serde(default)]
    pub description: String,
    pub kind: ChatProviderKind,
    pub model: String,
    /// Sampling temperature, or `None` to leave the field out of the request
    /// entirely. Newer models (Anthropic's Opus 4.7 generation onwards) reject
    /// `temperature` outright with a 400, so "send nothing" has to be
    /// expressible per profile, not just "send 0.0".
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Free-text thinking/reasoning value; empty means "no thinking".
    /// Interpreted per provider (OpenAI: `reasoning_effort`, so `low` /
    /// `medium` / `high`; Anthropic: a numeric token budget; Gemini: a numeric
    /// thinking budget). Kept as free text on purpose: providers keep adding
    /// values, and a fixed enum here would go stale. A value the provider
    /// cannot use surfaces as that provider's error.
    #[serde(default)]
    pub reasoning: String,
    /// How wordy the answer should be: OpenAI's `text.verbosity`, `low` /
    /// `medium` / `high`, empty for "do not send it". A separate knob from
    /// `reasoning` on purpose - that one buys thinking, this one buys prose,
    /// and the pair "think hard, answer briefly" is a common ask. Free text
    /// for the same reason `reasoning` is: the levels are the provider's to
    /// change. OpenAI only; the other providers ignore it.
    #[serde(default)]
    pub verbosity: String,
    /// Ask for OpenAI's Pro reasoning mode (`reasoning.mode: "pro"`): the
    /// slower, more thorough execution path, billed at the same per-token
    /// rate but spending more tokens. GPT-5.6 only, so it is off by default
    /// and a model that does not have it answers 400 rather than silently
    /// ignoring it.
    #[serde(default)]
    pub pro_mode: bool,
    /// Base URL, for OpenAI-compatible and Ollama profiles. Empty otherwise.
    #[serde(default)]
    pub base_url: String,
    /// When true this profile uses its own API key (stored under `id`) instead
    /// of the key shared by every profile of the same provider.
    #[serde(default)]
    pub use_own_key: bool,
    /// When true this profile's assistant may use the write tools (files,
    /// open tabs, writable database connections). Default false: a new
    /// profile is read-only until the user opts in. Replaces the global
    /// Write protection switch for the chat surface only (that switch still
    /// governs GUI file saves and the MCP server default).
    #[serde(default)]
    pub allow_writes: bool,
}

impl ChatModelProfile {
    /// A keyring-safe slug id derived from `name`, de-duplicated against the
    /// profiles that already exist.
    pub fn fresh_id(name: &str, existing: &[ChatModelProfile]) -> String {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        // Collapse runs of '-' so "Opus  (deep)" does not become "opus---deep-".
        let mut base = String::new();
        let mut last_dash = false;
        for c in slug.chars() {
            if c == '-' {
                if !last_dash {
                    base.push('-');
                }
                last_dash = true;
            } else {
                base.push(c);
                last_dash = false;
            }
        }
        let base = base.trim_matches('-');
        let base = if base.is_empty() { "profile" } else { base };

        let taken = |id: &str| existing.iter().any(|p| p.id == id);
        if !taken(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken(id))
            .unwrap_or_else(|| base.to_string())
    }
}

/// Build a profile from the legacy per-provider chat settings, so an existing
/// install keeps the exact provider, model, temperature and base URL it had.
///
/// The temperature comes along only when the install configured a model or
/// changed the temperature. A fresh install did neither and lands on the
/// default model, and the Anthropic default (Claude Haiku 5.5) answers 400 to
/// any `temperature` at all, as OpenAI's reasoning models do.
pub fn seed_profile_from_legacy(settings: &AppSettings) -> ChatModelProfile {
    let kind = settings.chat_provider;
    let legacy_model = settings
        .chat_models
        .get(kind.id())
        .filter(|m| !m.trim().is_empty())
        .cloned();
    let configured = legacy_model.is_some()
        || settings.chat_temperature != super::app_settings::default_chat_temperature();
    let temperature = configured.then_some(settings.chat_temperature);
    let model = legacy_model.unwrap_or_else(|| chat_models::default_model(kind));
    let base_url = match kind {
        ChatProviderKind::OpenAiCompatible => settings.chat_base_url.clone(),
        ChatProviderKind::Ollama => settings.chat_ollama_url.clone(),
        _ => String::new(),
    };
    ChatModelProfile {
        id: "default".to_string(),
        name: kind.label().to_string(),
        description: String::new(),
        kind,
        model,
        temperature,
        reasoning: String::new(),
        // Neither knob existed before profiles did, so a migrated install
        // starts with both off: same requests as it sent yesterday.
        verbosity: String::new(),
        pro_mode: false,
        base_url,
        use_own_key: false,
        // First migration keeps the behaviour the install had under the
        // global switch; profiles created later default to read-only.
        allow_writes: !settings.write_protection,
    }
}

/// Make sure the settings always carry at least one profile and a valid active
/// one. Runs on every settings load and after Settings-apply, and is idempotent:
///
/// - no profiles yet (fresh install or legacy settings): seed one from the old
///   per-provider fields and select it;
/// - active id pointing at a profile that no longer exists (the user deleted
///   it): fall back to the first profile rather than leaving the panel with no
///   model at all.
pub fn ensure_profiles(settings: &mut AppSettings) {
    if settings.chat_profiles.is_empty() {
        let p = seed_profile_from_legacy(settings);
        settings.chat_active_profile = p.id.clone();
        settings.chat_profiles.push(p);
    } else if !settings
        .chat_profiles
        .iter()
        .any(|p| p.id == settings.chat_active_profile)
    {
        settings.chat_active_profile = settings.chat_profiles[0].id.clone();
    }
}

/// The active profile, or the first one as a fallback. `ensure_profiles`
/// guarantees the list is non-empty in practice; the legacy seed covers the
/// theoretical empty case so this never has to return an `Option`.
pub fn active_profile(settings: &AppSettings) -> ChatModelProfile {
    settings
        .chat_profiles
        .iter()
        .find(|p| p.id == settings.chat_active_profile)
        .or_else(|| settings.chat_profiles.first())
        .cloned()
        .unwrap_or_else(|| seed_profile_from_legacy(settings))
}

#[cfg(test)]
#[path = "chat_profiles_tests.rs"]
mod tests;
