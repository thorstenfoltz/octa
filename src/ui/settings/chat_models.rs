//! Hand-editable runtime config for chat provider models.
//!
//! Lives beside `settings.toml` as `models.toml` so a user can add or remove
//! model names by hand without recompiling. On first run the file is written
//! seeded from the built-in lists compiled into [`ChatProviderKind`]; from then
//! on the file is the source of truth, except that built-in models added by a
//! newer release are merged in non-destructively on load (see
//! [`merge_and_order_presets`]). A provider (or an empty list) missing from the
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

/// Bring a parsed config up to date: every provider carries every built-in
/// model, and the list is ordered newest release first. Returns whether
/// anything changed (the caller rewrites the file only then).
///
/// Each provider's list is rebuilt as **the names Octa does not ship, in the
/// order the file already had, followed by the built-ins in built-in order**.
/// That single rule does every job: a built-in missing from the file is
/// added, built-ins the file already had are moved back into
/// [`ChatProviderKind::preset_models`] order (newest first), and the user's
/// own names stay.
///
/// Ordering the built-in list alone was not enough, which is the whole reason
/// this rewrites rather than appends. A `models.toml` grows by accretion:
/// every release appended its new models to the end, so a file seeded a year
/// ago listed the oldest model first and this year's flagship last. Only a
/// fresh install ever saw the intended order.
///
/// **Names Octa cannot date go on top**, because the reason to type a model
/// name Octa does not ship is that it is newer than the list: a model
/// announced this morning is exactly the one you want first, and burying it
/// under a two-year-old Haiku would defeat the point of adding it. The cost
/// is that a built-in dropped by a past release lands up there too and is
/// genuinely old. Deleting one sticks, unlike deleting a current built-in,
/// so a file that accumulated them can be tidied by hand once.
///
/// A non-empty user `default` stays authoritative.
///
/// Caveat: a built-in model the user deliberately deleted from the file is
/// re-added by this merge. Deleting built-ins is not a supported way to hide
/// them; the free-text model field always works regardless of the list.
fn merge_and_order_presets(cfg: &mut ChatModelsConfig) -> bool {
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
                let builtins = kind.preset_models();
                // The file's own names first, in the order it had them. The
                // `any` check also collapses a name duplicated by a hand-edit.
                let mut ordered: Vec<String> = Vec::new();
                for model in &entry.models {
                    if !builtins.iter().any(|p| p == model) && !ordered.iter().any(|o| o == model) {
                        ordered.push(model.clone());
                    }
                }
                ordered.extend(builtins.iter().map(|p| (*p).to_string()));
                if ordered != entry.models {
                    entry.models = ordered;
                    changed = true;
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
                if merge_and_order_presets(&mut cfg) {
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
#[path = "chat_models_tests.rs"]
mod tests;
