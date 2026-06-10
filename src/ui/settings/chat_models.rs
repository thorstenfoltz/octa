//! Hand-editable runtime config for chat provider models.
//!
//! Lives beside `settings.toml` as `models.toml` so a user can add or remove
//! model names by hand without recompiling. On first run the file is written
//! seeded from the built-in lists compiled into [`ChatProviderKind`]; from then
//! on the file is the source of truth, except that built-in models added by a
//! newer release are merged in non-destructively on load (see
//! [`merge_missing_presets`]). A provider (or an empty list) missing from the
//! file falls back to the built-in seed, so a hand-edit can never leave a
//! provider with no usable model.
//!
//! The parsed config is cached for the process lifetime; [`reload`] re-reads the
//! file (wired to a "Reload" button in the Settings dialog) so edits take effect
//! without restarting.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use super::{AppSettings, ChatProviderKind};

/// Per-provider model list: the default (free-text) model plus the quick-pick
/// presets shown in the dropdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderModels {
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub models: Vec<String>,
}

/// The whole `models.toml`: one entry per provider, keyed by
/// [`ChatProviderKind::id`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatModelsConfig {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderModels>,
}

static CACHE: RwLock<Option<ChatModelsConfig>> = RwLock::new(None);

fn models_toml_path() -> PathBuf {
    AppSettings::config_dir()
        .map(|d| d.join("models.toml"))
        .unwrap_or_else(|| PathBuf::from("models.toml"))
}

/// The user-visible path to `models.toml` (for the Settings dialog hint).
pub fn path() -> PathBuf {
    models_toml_path()
}

/// The built-in seed: the consts compiled into [`ChatProviderKind`]. Used to
/// write the default file and as the per-provider fallback.
fn seed() -> ChatModelsConfig {
    let mut providers = BTreeMap::new();
    for kind in ChatProviderKind::ALL {
        providers.insert(
            kind.id().to_string(),
            ProviderModels {
                default: kind.default_model().to_string(),
                models: kind.preset_models().iter().map(|s| s.to_string()).collect(),
            },
        );
    }
    ChatModelsConfig { providers }
}

/// Union the built-in presets into a parsed config so new built-in models
/// reach existing installs whose `models.toml` was seeded by an older
/// release. Missing providers are inserted wholesale from the seed; for
/// present providers, built-in models absent from the user's list are
/// appended at the end (user entries and their order are never touched, and
/// a non-empty user `default` stays authoritative). Returns whether anything
/// was added.
///
/// Caveat: a built-in model the user deliberately deleted from the file is
/// re-added by this merge. Deleting built-ins is not a supported way to hide
/// them; the free-text model field always works regardless of the list.
fn merge_missing_presets(cfg: &mut ChatModelsConfig) -> bool {
    let mut changed = false;
    for kind in ChatProviderKind::ALL {
        match cfg.providers.get_mut(kind.id()) {
            None => {
                cfg.providers.insert(
                    kind.id().to_string(),
                    ProviderModels {
                        default: kind.default_model().to_string(),
                        models: kind.preset_models().iter().map(|s| s.to_string()).collect(),
                    },
                );
                changed = true;
            }
            Some(entry) => {
                for preset in kind.preset_models() {
                    if !entry.models.iter().any(|m| m == preset) {
                        entry.models.push((*preset).to_string());
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

/// Best-effort write of the config to `models.toml`.
fn write_config(cfg: &ChatModelsConfig) {
    if let Some(dir) = AppSettings::config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(text) = toml::to_string_pretty(cfg) {
            let _ = std::fs::write(models_toml_path(), text);
        }
    }
}

/// Read `models.toml`, writing a seeded default when it is absent. A parsed
/// file gets new built-in presets merged in (and is rewritten only when that
/// added something). A malformed file is left untouched (we never clobber the
/// user's edits) and the built-in seed is used for that session.
fn load_or_create() -> ChatModelsConfig {
    let path = models_toml_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<ChatModelsConfig>(&text) {
            Ok(mut cfg) => {
                if merge_missing_presets(&mut cfg) {
                    write_config(&cfg);
                }
                cfg
            }
            Err(_) => seed(),
        },
        Err(_) => {
            let cfg = seed();
            write_config(&cfg);
            cfg
        }
    }
}

fn ensure_loaded() {
    if CACHE.read().unwrap().is_none() {
        let cfg = load_or_create();
        *CACHE.write().unwrap() = Some(cfg);
    }
}

/// Force a re-read of `models.toml` (e.g. after the user hand-edits it).
pub fn reload() {
    let cfg = load_or_create();
    *CACHE.write().unwrap() = Some(cfg);
}

/// The quick-pick preset models for a provider. Falls back to the built-in
/// list when the provider is absent from the file or its list is empty.
pub fn preset_models(kind: ChatProviderKind) -> Vec<String> {
    ensure_loaded();
    let guard = CACHE.read().unwrap();
    guard
        .as_ref()
        .and_then(|c| c.providers.get(kind.id()))
        .map(|p| p.models.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| kind.preset_models().iter().map(|s| s.to_string()).collect())
}

/// The default model for a provider. Falls back to the built-in default when
/// the file has no (non-empty) entry.
pub fn default_model(kind: ChatProviderKind) -> String {
    ensure_loaded();
    let guard = CACHE.read().unwrap();
    guard
        .as_ref()
        .and_then(|c| c.providers.get(kind.id()))
        .map(|p| p.default.clone())
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| kind.default_model().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old_anthropic_config() -> ChatModelsConfig {
        // A models.toml seeded by a release that predates claude-fable-5,
        // with one user-added custom model and a user-chosen default.
        let mut providers = BTreeMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderModels {
                default: "my-custom-default".to_string(),
                models: vec![
                    "claude-sonnet-4-6".to_string(),
                    "claude-opus-4-8".to_string(),
                    "my-custom-model".to_string(),
                ],
            },
        );
        ChatModelsConfig { providers }
    }

    #[test]
    fn merge_adds_new_builtin_models_without_touching_user_entries() {
        let mut cfg = old_anthropic_config();
        assert!(merge_missing_presets(&mut cfg));
        let anthropic = &cfg.providers["anthropic"];
        // User entries keep their order at the front.
        assert_eq!(anthropic.models[0], "claude-sonnet-4-6");
        assert_eq!(anthropic.models[1], "claude-opus-4-8");
        assert_eq!(anthropic.models[2], "my-custom-model");
        // New built-ins are appended.
        assert!(anthropic.models.iter().any(|m| m == "claude-fable-5"));
        assert!(
            anthropic
                .models
                .iter()
                .any(|m| m == "claude-haiku-4-5-20251001")
        );
        // The user's default is never touched.
        assert_eq!(anthropic.default, "my-custom-default");
    }

    #[test]
    fn merge_seeds_missing_providers_wholesale() {
        let mut cfg = old_anthropic_config();
        merge_missing_presets(&mut cfg);
        for kind in ChatProviderKind::ALL {
            let entry = cfg
                .providers
                .get(kind.id())
                .unwrap_or_else(|| panic!("provider {} missing after merge", kind.id()));
            if kind.id() != "anthropic" {
                assert_eq!(entry.default, kind.default_model());
            }
        }
    }

    #[test]
    fn merge_is_idempotent() {
        let mut cfg = old_anthropic_config();
        assert!(merge_missing_presets(&mut cfg));
        let snapshot = format!("{cfg:?}");
        assert!(
            !merge_missing_presets(&mut cfg),
            "second merge must add nothing"
        );
        assert_eq!(format!("{cfg:?}"), snapshot);
    }
}
