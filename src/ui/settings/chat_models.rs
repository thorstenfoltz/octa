//! Hand-editable runtime config for chat provider models.
//!
//! Lives beside `settings.toml` as `models.toml` so a user can add or remove
//! model names by hand without recompiling. On first run the file is written
//! seeded from the built-in lists compiled into [`ChatProviderKind`]; from then
//! on the file is the source of truth. A provider (or an empty list) missing
//! from the file falls back to the built-in seed, so a hand-edit can never
//! leave a provider with no usable model.
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

/// Read `models.toml`, writing a seeded default when it is absent. A malformed
/// file is left untouched (we never clobber the user's edits) and the built-in
/// seed is used for that session.
fn load_or_create() -> ChatModelsConfig {
    let path = models_toml_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<ChatModelsConfig>(&text).unwrap_or_else(|_| seed()),
        Err(_) => {
            let cfg = seed();
            if let Some(dir) = AppSettings::config_dir() {
                let _ = std::fs::create_dir_all(&dir);
                if let Ok(text) = toml::to_string_pretty(&cfg) {
                    let _ = std::fs::write(&path, text);
                }
            }
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
