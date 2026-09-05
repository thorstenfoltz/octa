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
    // Honour the same OCTA_NO_KEYRING escape hatch as the chat keys, so a
    // container without D-Bus takes the plaintext path immediately rather
    // than waiting for a Secret Service lookup to fail.
    if super::secrets::keyring_disabled() {
        return Err(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("keyring disabled by OCTA_NO_KEYRING"),
        )));
    }
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

/// The SSH-tunnel credential lives in its own entry: a connection can need
/// both a database password and a bastion password at once, so one opaque
/// string per connection is not enough here.
///
/// Meaning depends on the tunnel's auth mode: the private key's passphrase for
/// `PrivateKey`, the account password for `Password`, and nothing at all for
/// `Agent`.
fn ssh_entry(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    if super::secrets::keyring_disabled() {
        return Err(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::other("keyring disabled by OCTA_NO_KEYRING"),
        )));
    }
    keyring::Entry::new(KEYRING_SERVICE, &format!("db.{connection_id}.ssh"))
}

/// Key under which the plaintext fallback stores the SSH credential, kept in
/// the same map as the database secret so no second settings field is needed.
fn ssh_fallback_key(connection_id: &str) -> String {
    format!("{connection_id}.ssh")
}

/// Read a connection's SSH-tunnel credential: keyring first, then the
/// plaintext map, exactly like [`get_db_secret`].
pub fn get_ssh_secret(connection_id: &str, settings: &AppSettings) -> Option<String> {
    if let Ok(entry) = ssh_entry(connection_id)
        && let Ok(pw) = entry.get_password()
        && !pw.trim().is_empty()
    {
        return Some(pw);
    }
    settings
        .db_secrets
        .get(&ssh_fallback_key(connection_id))
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Store a connection's SSH-tunnel credential. `Ok(true)` = keyring,
/// `Ok(false)` = plaintext fallback (the caller persists settings).
pub fn set_ssh_secret(
    connection_id: &str,
    secret: &str,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    match ssh_entry(connection_id).and_then(|e| e.set_password(secret)) {
        Ok(()) => {
            settings.db_secrets.remove(&ssh_fallback_key(connection_id));
            Ok(true)
        }
        Err(_) => {
            settings
                .db_secrets
                .insert(ssh_fallback_key(connection_id), secret.to_string());
            Ok(false)
        }
    }
}

/// Delete a connection's SSH-tunnel credential from both stores.
pub fn delete_ssh_secret(connection_id: &str, settings: &mut AppSettings) {
    if let Ok(entry) = ssh_entry(connection_id) {
        let _ = entry.delete_credential();
    }
    settings.db_secrets.remove(&ssh_fallback_key(connection_id));
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
    fn the_ssh_credential_does_not_collide_with_the_database_one() {
        // A connection behind a bastion needs both at once. Sharing one entry
        // would silently overwrite the database password with the SSH one.
        let mut settings = AppSettings::default();
        settings
            .db_secrets
            .insert("db-both".to_string(), "db-password".to_string());
        settings
            .db_secrets
            .insert("db-both.ssh".to_string(), "bastion-password".to_string());
        assert_eq!(
            get_db_secret("db-both", &settings).as_deref(),
            Some("db-password")
        );
        assert_eq!(
            get_ssh_secret("db-both", &settings).as_deref(),
            Some("bastion-password")
        );
        // Clearing one leaves the other alone.
        delete_ssh_secret("db-both", &mut settings);
        assert_eq!(get_ssh_secret("db-both", &settings), None);
        assert_eq!(
            get_db_secret("db-both", &settings).as_deref(),
            Some("db-password")
        );
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
