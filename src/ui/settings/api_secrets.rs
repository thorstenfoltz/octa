//! Per-connection API credential storage, mirroring [`super::db_secrets`]:
//! keyring first, plaintext `api_secrets` fallback, never hard-fails.
//!
//! The `api.<id>.secret` entry holds one opaque string whose meaning depends
//! on the connection's auth kind - a bearer token, an API key, or the password
//! half of basic auth. One shape, so no JSON wrapper. An endpoint with
//! `ApiAuth::None` stores nothing.

use super::AppSettings;
use super::secrets::KeyStorage;

const KEYRING_SERVICE: &str = "octa";

fn keyring_entry(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    // The same OCTA_NO_KEYRING escape hatch the other secret stores honour, so
    // a container without D-Bus takes the plaintext path immediately rather
    // than waiting for a Secret Service lookup to fail.
    if super::secrets::keyring_disabled() {
        return Err(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("keyring disabled by OCTA_NO_KEYRING"),
        )));
    }
    keyring::Entry::new(KEYRING_SERVICE, &format!("api.{connection_id}.secret"))
}

/// Read a connection's stored credential: keyring first, then the plaintext map.
pub fn get_api_secret(connection_id: &str, settings: &AppSettings) -> Option<String> {
    if let Ok(entry) = keyring_entry(connection_id)
        && let Ok(v) = entry.get_password()
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    settings
        .api_secrets
        .get(connection_id)
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Store a credential. Keyring first; on failure fall back to the plaintext
/// map (the caller persists settings). `Ok(true)` = keyring, `Ok(false)` =
/// plaintext.
pub fn set_api_secret(
    connection_id: &str,
    secret: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    match keyring_entry(connection_id).and_then(|e| e.set_password(secret)) {
        Ok(()) => {
            settings.api_secrets.remove(connection_id);
            Ok(true)
        }
        Err(_) => {
            settings
                .api_secrets
                .insert(connection_id.to_string(), secret.to_string());
            Ok(false)
        }
    }
}

/// Delete a credential from both the keyring and the plaintext map.
pub fn delete_api_secret(connection_id: &str, settings: &mut AppSettings) {
    if let Ok(entry) = keyring_entry(connection_id) {
        let _ = entry.delete_credential();
    }
    settings.api_secrets.remove(connection_id);
}

/// Where this connection's credential actually lives, for the UI badge that
/// warns when it fell back to plaintext.
pub fn api_secret_storage(connection_id: &str, settings: &AppSettings) -> KeyStorage {
    if let Ok(entry) = keyring_entry(connection_id)
        && let Ok(v) = entry.get_password()
        && !v.trim().is_empty()
    {
        return KeyStorage::Keyring;
    }
    if settings
        .api_secrets
        .get(connection_id)
        .is_some_and(|v| !v.trim().is_empty())
    {
        return KeyStorage::Plaintext(super::secrets::plaintext_path());
    }
    KeyStorage::None
}
