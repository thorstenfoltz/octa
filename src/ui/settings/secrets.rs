//! Chat API-key storage with a strict precedence and a never-fail fallback
//! chain: **environment variable -> OS keyring -> plaintext settings.toml**. A
//! missing or headless keyring never hard-errors; it just falls through. The UI
//! uses [`storage_location`] to tell the user exactly where each key lives.
//!
//! Keys are addressed by an opaque **key id**, which is one of two things:
//!
//! - a **provider** key ([`ChatProviderKind::id`], e.g. `anthropic`), shared by
//!   every profile of that provider. This is the default and the common case.
//! - a **profile** key, for a profile that opted into `use_own_key`. These are
//!   namespaced `profile.<profile-id>` so that a profile the user happens to
//!   name "Anthropic" cannot collide with the shared Anthropic key.
//!
//! Only provider keys have an environment variable; a profile key is explicit
//! by definition, so env lookup is skipped for them.
//!
//! Lives in the library (next to [`AppSettings`]) rather than the binary's chat
//! module so both the chat panel and the library-owned Settings dialog can
//! manage keys; the binary re-exports it as `crate::app::chat::secrets`.

use std::path::PathBuf;

use super::{AppSettings, ChatProviderKind};

const KEYRING_SERVICE: &str = "octa";

/// Where a key is (or would be) read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyStorage {
    /// Supplied via the provider's environment variable (read-only here).
    Env(String),
    /// Stored securely in the OS keyring.
    Keyring,
    /// Stored in plaintext at the given `settings.toml` path (keyring absent).
    Plaintext(PathBuf),
    /// No key configured anywhere.
    None,
}

/// The key id under which a profile's own key is stored. Namespaced so it can
/// never collide with a provider id.
pub fn profile_key_id(profile_id: &str) -> String {
    format!("profile.{profile_id}")
}

/// Whether the OS keyring should be skipped entirely, from `OCTA_NO_KEYRING`.
///
/// The fallback to plaintext already triggers when the keyring *errors*, which
/// covers most headless setups. This exists for the case it does not: a
/// container with no D-Bus session where the Secret Service lookup is slow to
/// fail rather than instant, on a code path (`octa --mcp`) that has no UI to
/// show a spinner. Set `OCTA_NO_KEYRING=1` and every secret goes to and comes
/// from `settings.toml` directly.
///
/// Read once: the environment does not change under a running process, and
/// this sits in front of every secret read.
pub fn keyring_disabled() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| {
        std::env::var("OCTA_NO_KEYRING")
            .map(|v| {
                let v = v.trim().to_ascii_lowercase();
                !v.is_empty() && v != "0" && v != "false" && v != "no"
            })
            .unwrap_or(false)
    })
}

fn keyring_entry(key_id: &str) -> Result<keyring::Entry, keyring::Error> {
    if keyring_disabled() {
        return Err(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("keyring disabled by OCTA_NO_KEYRING"),
        )));
    }
    keyring::Entry::new(KEYRING_SERVICE, &format!("chat.{key_id}.api_key"))
}

fn settings_toml_path() -> PathBuf {
    AppSettings::config_dir()
        .map(|d| d.join("settings.toml"))
        .unwrap_or_else(|| PathBuf::from("settings.toml"))
}

// ---------------------------------------------------------------------------
// Generic, key-id-based API. The per-provider and per-profile functions below
// are thin wrappers that pick the right id (and whether an env var applies).
// ---------------------------------------------------------------------------

/// Resolve the usable key for a key id, honouring the precedence chain.
/// `env_var` is consulted first when given (provider keys only).
pub fn get_key_for(key_id: &str, env_var: Option<&str>, settings: &AppSettings) -> Option<String> {
    if let Some(var) = env_var
        && let Ok(v) = std::env::var(var)
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    if let Ok(entry) = keyring_entry(key_id)
        && let Ok(pw) = entry.get_password()
        && !pw.trim().is_empty()
    {
        return Some(pw);
    }
    settings
        .chat_api_keys
        .get(key_id)
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Report where a key id currently resolves from (for the Settings status
/// line), without returning the secret itself.
pub fn key_storage_for(key_id: &str, env_var: Option<&str>, settings: &AppSettings) -> KeyStorage {
    if let Some(var) = env_var
        && std::env::var(var)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    {
        return KeyStorage::Env(var.to_string());
    }
    if let Ok(entry) = keyring_entry(key_id)
        && entry
            .get_password()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false)
    {
        return KeyStorage::Keyring;
    }
    if settings
        .chat_api_keys
        .get(key_id)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return KeyStorage::Plaintext(settings_toml_path());
    }
    KeyStorage::None
}

/// Store a key under a key id. Tries the keyring first; on any keyring failure
/// falls back to the plaintext `chat_api_keys` map in settings (the caller
/// persists settings). Returns `Ok(true)` when the key landed in the keyring,
/// `Ok(false)` when it fell back to plaintext (so the UI can warn + show the
/// path).
pub fn set_key_for(key_id: &str, key: &str, settings: &mut AppSettings) -> Result<bool, String> {
    match keyring_entry(key_id).and_then(|e| e.set_password(key)) {
        Ok(()) => {
            // Keyring won; drop any stale plaintext copy.
            settings.chat_api_keys.remove(key_id);
            Ok(true)
        }
        Err(_) => {
            settings
                .chat_api_keys
                .insert(key_id.to_string(), key.to_string());
            Ok(false)
        }
    }
}

/// Remove a key from both the keyring and the plaintext fallback.
pub fn delete_key_for(key_id: &str, settings: &mut AppSettings) {
    if let Ok(entry) = keyring_entry(key_id) {
        let _ = entry.delete_credential();
    }
    settings.chat_api_keys.remove(key_id);
}

// ---------------------------------------------------------------------------
// Per-provider keys (the shared default).
// ---------------------------------------------------------------------------

/// Resolve the usable key for a provider, honouring the precedence chain.
pub fn get_api_key(kind: ChatProviderKind, settings: &AppSettings) -> Option<String> {
    get_key_for(kind.id(), Some(kind.env_var()), settings)
}

/// Report where the provider's key currently resolves from.
pub fn storage_location(kind: ChatProviderKind, settings: &AppSettings) -> KeyStorage {
    key_storage_for(kind.id(), Some(kind.env_var()), settings)
}

/// Store a provider's key. See [`set_key_for`] for the return value.
pub fn set_api_key(
    kind: ChatProviderKind,
    key: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    set_key_for(kind.id(), key, settings)
}

/// Remove a provider's stored key.
pub fn delete_api_key(kind: ChatProviderKind, settings: &mut AppSettings) {
    delete_key_for(kind.id(), settings);
}

// ---------------------------------------------------------------------------
// Per-profile keys (the `use_own_key` override).
// ---------------------------------------------------------------------------

/// Resolve a profile's own key. No environment variable applies: a profile key
/// is an explicit override, and the env var already backs the shared key.
pub fn get_profile_key(profile_id: &str, settings: &AppSettings) -> Option<String> {
    get_key_for(&profile_key_id(profile_id), None, settings)
}

/// Report where a profile's own key lives.
pub fn profile_key_storage(profile_id: &str, settings: &AppSettings) -> KeyStorage {
    key_storage_for(&profile_key_id(profile_id), None, settings)
}

/// Store a profile's own key.
pub fn set_profile_key(
    profile_id: &str,
    key: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    set_key_for(&profile_key_id(profile_id), key, settings)
}

/// Remove a profile's own key (called when the profile is deleted, or when the
/// user turns `use_own_key` back off).
pub fn delete_profile_key(profile_id: &str, settings: &mut AppSettings) {
    delete_key_for(&profile_key_id(profile_id), settings);
}

/// The settings.toml path, for UI messages about plaintext storage.
pub fn plaintext_path() -> PathBuf {
    settings_toml_path()
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
