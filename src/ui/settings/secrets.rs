//! Per-provider API-key storage with a strict precedence and a never-fail
//! fallback chain: **environment variable -> OS keyring -> plaintext
//! settings.toml**. A missing or headless keyring never hard-errors; it just
//! falls through. The UI uses [`storage_location`] to tell the user exactly
//! where each key lives.
//!
//! Lives in the library (next to [`AppSettings`]) rather than the binary's chat
//! module so both the chat panel and the library-owned Settings dialog can
//! manage keys; the binary re-exports it as `crate::app::chat::secrets`.

use std::path::PathBuf;

use super::{AppSettings, ChatProviderKind};

const KEYRING_SERVICE: &str = "octa";

/// Where a provider's key is (or would be) read from.
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

fn keyring_entry(kind: ChatProviderKind) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("chat.{}.api_key", kind.id()))
}

fn settings_toml_path() -> PathBuf {
    AppSettings::config_dir()
        .map(|d| d.join("settings.toml"))
        .unwrap_or_else(|| PathBuf::from("settings.toml"))
}

/// Resolve the usable key for a provider, honouring the precedence chain.
pub fn get_api_key(kind: ChatProviderKind, settings: &AppSettings) -> Option<String> {
    if let Ok(v) = std::env::var(kind.env_var())
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    if let Ok(entry) = keyring_entry(kind)
        && let Ok(pw) = entry.get_password()
        && !pw.trim().is_empty()
    {
        return Some(pw);
    }
    settings
        .chat_api_keys
        .get(kind.id())
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Report where the provider's key currently resolves from (for the Settings
/// status line), without returning the secret itself.
pub fn storage_location(kind: ChatProviderKind, settings: &AppSettings) -> KeyStorage {
    if std::env::var(kind.env_var())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return KeyStorage::Env(kind.env_var().to_string());
    }
    if let Ok(entry) = keyring_entry(kind)
        && entry
            .get_password()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false)
    {
        return KeyStorage::Keyring;
    }
    if settings
        .chat_api_keys
        .get(kind.id())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return KeyStorage::Plaintext(settings_toml_path());
    }
    KeyStorage::None
}

/// Store a key. Tries the keyring first; on any keyring failure falls back to
/// the plaintext `chat_api_keys` map in settings (the caller persists
/// settings). Returns `Ok(true)` when the key landed in the keyring, `Ok(false)`
/// when it fell back to plaintext (so the UI can warn + show the path).
pub fn set_api_key(
    kind: ChatProviderKind,
    key: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    match keyring_entry(kind).and_then(|e| e.set_password(key)) {
        Ok(()) => {
            // Keyring won; drop any stale plaintext copy.
            settings.chat_api_keys.remove(kind.id());
            Ok(true)
        }
        Err(_) => {
            settings
                .chat_api_keys
                .insert(kind.id().to_string(), key.to_string());
            Ok(false)
        }
    }
}

/// Remove a stored key from both the keyring and the plaintext fallback.
pub fn delete_api_key(kind: ChatProviderKind, settings: &mut AppSettings) {
    if let Ok(entry) = keyring_entry(kind) {
        let _ = entry.delete_credential();
    }
    settings.chat_api_keys.remove(kind.id());
}

/// The settings.toml path, for UI messages about plaintext storage.
pub fn plaintext_path() -> PathBuf {
    settings_toml_path()
}
