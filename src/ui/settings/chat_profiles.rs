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
    #[serde(default)]
    pub temperature: f32,
    /// Free-text thinking/reasoning value; empty means "no thinking".
    /// Interpreted per provider (OpenAI: `reasoning_effort`, so `low` /
    /// `medium` / `high`; Anthropic: a numeric token budget; Gemini: a numeric
    /// thinking budget). Kept as free text on purpose: providers keep adding
    /// values, and a fixed enum here would go stale. A value the provider
    /// cannot use surfaces as that provider's error.
    #[serde(default)]
    pub reasoning: String,
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
pub fn seed_profile_from_legacy(settings: &AppSettings) -> ChatModelProfile {
    let kind = settings.chat_provider;
    let model = settings
        .chat_models
        .get(kind.id())
        .filter(|m| !m.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| chat_models::default_model(kind));
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
        temperature: settings.chat_temperature,
        reasoning: String::new(),
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
mod tests {
    use super::*;

    #[test]
    fn ensure_profiles_seeds_one_from_legacy() {
        let mut s = AppSettings {
            chat_provider: ChatProviderKind::Anthropic,
            chat_temperature: 0.7,
            ..Default::default()
        };
        s.chat_models
            .insert("anthropic".into(), "claude-opus-4-8".into());
        assert!(s.chat_profiles.is_empty());

        ensure_profiles(&mut s);

        assert_eq!(s.chat_profiles.len(), 1);
        assert_eq!(s.chat_active_profile, "default");
        assert_eq!(s.chat_profiles[0].kind, ChatProviderKind::Anthropic);
        assert_eq!(s.chat_profiles[0].model, "claude-opus-4-8");
        assert_eq!(s.chat_profiles[0].temperature, 0.7);
    }

    #[test]
    fn seed_carries_the_ollama_base_url() {
        let mut s = AppSettings {
            chat_provider: ChatProviderKind::Ollama,
            chat_ollama_url: "http://localhost:11434".into(),
            ..Default::default()
        };
        ensure_profiles(&mut s);
        assert_eq!(s.chat_profiles[0].base_url, "http://localhost:11434");
    }

    #[test]
    fn ensure_profiles_is_idempotent() {
        let mut s = AppSettings::default();
        ensure_profiles(&mut s);
        let first = s.chat_profiles.clone();
        ensure_profiles(&mut s);
        ensure_profiles(&mut s);
        assert_eq!(s.chat_profiles, first);
    }

    #[test]
    fn fresh_id_slugifies_and_dedupes() {
        let mut existing: Vec<ChatModelProfile> = Vec::new();
        let id = ChatModelProfile::fresh_id("Opus (deep)", &existing);
        assert_eq!(id, "opus-deep");

        existing.push(ChatModelProfile {
            id,
            name: "Opus (deep)".into(),
            description: String::new(),
            kind: ChatProviderKind::Anthropic,
            model: "claude-opus-4-8".into(),
            temperature: 0.0,
            reasoning: String::new(),
            base_url: String::new(),
            use_own_key: false,
            allow_writes: false,
        });

        // Same name again gets a distinct id, so the two profiles' own-keys
        // never collide in the keyring.
        assert_eq!(
            ChatModelProfile::fresh_id("Opus (deep)", &existing),
            "opus-deep-2"
        );
    }

    #[test]
    fn fresh_id_survives_a_nameless_profile() {
        let existing: Vec<ChatModelProfile> = Vec::new();
        assert_eq!(ChatModelProfile::fresh_id("...", &existing), "profile");
    }

    #[test]
    fn dangling_active_falls_back_to_first() {
        let mut s = AppSettings::default();
        ensure_profiles(&mut s);
        s.chat_active_profile = "deleted-profile".into();

        ensure_profiles(&mut s);

        assert_eq!(s.chat_active_profile, s.chat_profiles[0].id);
    }

    #[test]
    fn settings_with_profiles_round_trip_through_toml() {
        // `chat_profiles` is a Vec<struct>, which TOML writes as an array of
        // tables. `AppSettings::save` swallows a serialisation error
        // (`if let Ok(..)`), so a struct-ordering mistake here would silently
        // stop settings being saved at all rather than failing loudly. Pin it.
        let mut s = AppSettings::default();
        ensure_profiles(&mut s);
        s.chat_profiles.push(ChatModelProfile {
            id: "opus-deep".into(),
            name: "Opus deep".into(),
            description: "for the hard questions".into(),
            kind: ChatProviderKind::Anthropic,
            model: "claude-opus-4-8".into(),
            temperature: 0.3,
            reasoning: "8000".into(),
            base_url: String::new(),
            use_own_key: true,
            allow_writes: true,
        });
        s.chat_active_profile = "opus-deep".into();

        let text = toml::to_string_pretty(&s).expect("settings must serialise");
        let back: AppSettings = toml::from_str(&text).expect("settings must parse back");

        assert_eq!(back.chat_profiles.len(), 2);
        assert_eq!(back.chat_active_profile, "opus-deep");
        let p = back
            .chat_profiles
            .iter()
            .find(|p| p.id == "opus-deep")
            .expect("the profile survives the round trip");
        assert_eq!(p.kind, ChatProviderKind::Anthropic);
        assert_eq!(p.temperature, 0.3);
        assert_eq!(p.reasoning, "8000");
        assert_eq!(p.description, "for the hard questions");
        assert!(p.use_own_key);
        assert!(p.allow_writes);
    }

    #[test]
    fn legacy_seed_inherits_the_global_write_switch() {
        // The first migration keeps the behaviour the install already had:
        // write protection OFF meant the assistant could write, so the seeded
        // profile allows writes; protection ON seeds a read-only profile.
        for (write_protection, expect_allow) in [(false, true), (true, false)] {
            let mut s = AppSettings {
                write_protection,
                ..Default::default()
            };
            ensure_profiles(&mut s);
            assert_eq!(s.chat_profiles[0].allow_writes, expect_allow);
        }
    }

    #[test]
    fn legacy_settings_without_profiles_still_parse() {
        // An existing settings.toml has no [[chat_profiles]] at all. It must
        // load (serde default) and then migrate, not error out into defaults.
        let legacy = r#"
            chat_provider = "OpenAi"
            chat_temperature = 0.5
        "#;
        let mut s: AppSettings = toml::from_str(legacy).expect("legacy settings parse");
        assert!(s.chat_profiles.is_empty());

        ensure_profiles(&mut s);

        assert_eq!(s.chat_profiles.len(), 1);
        assert_eq!(s.chat_profiles[0].kind, ChatProviderKind::OpenAi);
        assert_eq!(s.chat_profiles[0].temperature, 0.5);
    }

    #[test]
    fn active_profile_resolves_the_selected_one() {
        let mut s = AppSettings::default();
        ensure_profiles(&mut s);
        s.chat_profiles.push(ChatModelProfile {
            id: "sonnet-quick".into(),
            name: "Sonnet quick".into(),
            description: String::new(),
            kind: ChatProviderKind::Anthropic,
            model: "claude-sonnet-5".into(),
            temperature: 0.2,
            reasoning: String::new(),
            base_url: String::new(),
            use_own_key: false,
            allow_writes: false,
        });
        s.chat_active_profile = "sonnet-quick".into();

        let p = active_profile(&s);

        assert_eq!(p.name, "Sonnet quick");
        assert_eq!(p.model, "claude-sonnet-5");
    }
}
