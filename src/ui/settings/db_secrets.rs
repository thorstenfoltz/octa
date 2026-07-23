//! Per-connection database secret storage, mirroring [`super::cloud_secrets`]:
//! keyring first, plaintext `db_secrets` fallback, never hard-fails. The
//! `db.<id>.secret` entry holds one opaque string whose meaning depends on the
//! connection's auth kind - a password, an IAM/AD token is minted per connect
//! (nothing stored), a Databricks personal access token, a Snowflake key
//! passphrase, or an OAuth client secret. One shape, so no JSON wrapper.
//! The separate `db.<id>.oauth` entry (see [`get_oauth_cache`]) caches a
//! short-lived OAuth access token.

use super::AppSettings;
use super::secrets::KeyStorage;

const KEYRING_SERVICE: &str = "octa";

fn keyring_entry(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("db.{connection_id}.secret"))
}

/// Read a connection's stored secret: keyring first, then the plaintext map.
pub fn get_db_secret(connection_id: &str, settings: &AppSettings) -> Option<String> {
    if let Ok(entry) = keyring_entry(connection_id)
        && let Ok(pw) = entry.get_password()
        && !pw.trim().is_empty()
    {
        return Some(pw);
    }
    settings
        .db_secrets
        .get(connection_id)
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Store a secret. Keyring first; on failure fall back to the plaintext map
/// (the caller persists settings). `Ok(true)` = keyring, `Ok(false)` = plaintext.
pub fn set_db_secret(
    connection_id: &str,
    secret: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    match keyring_entry(connection_id).and_then(|e| e.set_password(secret)) {
        Ok(()) => {
            settings.db_secrets.remove(connection_id);
            Ok(true)
        }
        Err(_) => {
            settings
                .db_secrets
                .insert(connection_id.to_string(), secret.to_string());
            Ok(false)
        }
    }
}

/// Delete a connection's secret from both the keyring and the plaintext map.
pub fn delete_db_secret(connection_id: &str, settings: &mut AppSettings) {
    if let Ok(entry) = keyring_entry(connection_id) {
        let _ = entry.delete_credential();
    }
    settings.db_secrets.remove(connection_id);
}

fn oauth_cache_entry(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("db.{connection_id}.oauth"))
}

/// Read a connection's cached OAuth token (opaque JSON). Keyring-only: the
/// cache is a best-effort optimisation, so a missing keyring just means the
/// token gets re-minted.
pub fn get_oauth_cache(connection_id: &str) -> Option<String> {
    oauth_cache_entry(connection_id)
        .ok()?
        .get_password()
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// Store a connection's cached OAuth token JSON (best-effort keyring write).
pub fn set_oauth_cache(connection_id: &str, json: &str) {
    if let Ok(entry) = oauth_cache_entry(connection_id) {
        let _ = entry.set_password(json);
    }
}

/// Drop a connection's cached OAuth token.
pub fn delete_oauth_cache(connection_id: &str) {
    if let Ok(entry) = oauth_cache_entry(connection_id) {
        let _ = entry.delete_credential();
    }
}

/// Where a connection's secret currently resolves from (for the Settings UI).
pub fn db_secret_storage(connection_id: &str, settings: &AppSettings) -> KeyStorage {
    if let Ok(entry) = keyring_entry(connection_id)
        && entry
            .get_password()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false)
    {
        return KeyStorage::Keyring;
    }
    if settings
        .db_secrets
        .get(connection_id)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return KeyStorage::Plaintext(super::secrets::plaintext_path());
    }
    KeyStorage::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_fallback_roundtrip() {
        let mut settings = AppSettings::default();
        // Force the plaintext path deterministically (the keyring may or may
        // not exist on CI): write straight into the fallback map, then read.
        settings
            .db_secrets
            .insert("db-test-xyz".to_string(), "s3cret".to_string());
        assert_eq!(
            get_db_secret("db-test-xyz", &settings).as_deref(),
            Some("s3cret")
        );
        delete_db_secret("db-test-xyz", &mut settings);
        assert_eq!(get_db_secret("db-test-xyz", &settings), None);
    }

    #[test]
    fn empty_plaintext_secret_is_none() {
        let mut settings = AppSettings::default();
        settings
            .db_secrets
            .insert("db-empty".to_string(), "  ".to_string());
        assert_eq!(get_db_secret("db-empty", &settings), None);
    }

    #[test]
    fn one_entry_serves_every_auth_kind() {
        // The same db.<id>.secret entry stores a PAT, a key passphrase, or an
        // OAuth client secret interchangeably - it is just an opaque string.
        let mut settings = AppSettings::default();
        for secret in ["dapiPAT123", "keyPassphrase!", "oauth-client-secret"] {
            settings
                .db_secrets
                .insert("db-multi".to_string(), secret.to_string());
            assert_eq!(
                get_db_secret("db-multi", &settings).as_deref(),
                Some(secret)
            );
        }
        delete_db_secret("db-multi", &mut settings);
        assert_eq!(get_db_secret("db-multi", &settings), None);
    }
}
