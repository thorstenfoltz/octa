//! Per-connection cloud secret storage, mirroring [`super::secrets`]: keyring
//! first, plaintext `cloud_secrets` fallback, never hard-fails. Includes
//! deleting a stored secret (the cloud analogue of `delete_api_key`).

use super::AppSettings;
use super::secrets::KeyStorage;
use crate::cloud::{CloudConnection, CloudSecret, ProviderCreds, resolve_ambient_creds};

const KEYRING_SERVICE: &str = "octa";

fn keyring_entry(connection_id: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("cloud.{connection_id}.secret"))
}

/// Read a connection's stored secret: keyring first, then the plaintext map.
pub fn get_cloud_secret(connection_id: &str, settings: &AppSettings) -> Option<CloudSecret> {
    if let Ok(entry) = keyring_entry(connection_id)
        && let Ok(pw) = entry.get_password()
        && !pw.trim().is_empty()
        && let Ok(secret) = serde_json::from_str::<CloudSecret>(&pw)
    {
        return Some(secret);
    }
    settings
        .cloud_secrets
        .get(connection_id)
        .and_then(|v| serde_json::from_str::<CloudSecret>(v).ok())
}

/// Store a secret. Keyring first; on failure fall back to the plaintext map
/// (the caller persists settings). `Ok(true)` = keyring, `Ok(false)` = plaintext.
pub fn set_cloud_secret(
    connection_id: &str,
    secret: &CloudSecret,
    settings: &mut AppSettings,
) -> Result<bool, String> {
    let json = serde_json::to_string(secret).map_err(|e| e.to_string())?;
    match keyring_entry(connection_id).and_then(|e| e.set_password(&json)) {
        Ok(()) => {
            settings.cloud_secrets.remove(connection_id);
            Ok(true)
        }
        Err(_) => {
            settings
                .cloud_secrets
                .insert(connection_id.to_string(), json);
            Ok(false)
        }
    }
}

/// Delete a connection's secret from both the keyring and the plaintext map.
pub fn delete_cloud_secret(connection_id: &str, settings: &mut AppSettings) {
    if let Ok(entry) = keyring_entry(connection_id) {
        let _ = entry.delete_credential();
    }
    settings.cloud_secrets.remove(connection_id);
}

/// Where a connection's secret currently resolves from (for the Settings UI).
pub fn cloud_secret_storage(connection_id: &str, settings: &AppSettings) -> KeyStorage {
    if let Ok(entry) = keyring_entry(connection_id)
        && entry
            .get_password()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false)
    {
        return KeyStorage::Keyring;
    }
    if settings
        .cloud_secrets
        .get(connection_id)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return KeyStorage::Plaintext(super::secrets::plaintext_path());
    }
    KeyStorage::None
}

/// Resolve the credentials to build a provider for `conn`: a stored secret if
/// present, otherwise the ambient environment / CLI / ADC chain.
pub fn resolve_creds(conn: &CloudConnection, settings: &AppSettings) -> ProviderCreds {
    // A cached browser token wins (checked first: a stored GCS OAuth client
    // secret resolves to Ambient, not a usable credential, so it must not
    // shadow the token).
    if let Some(t) = crate::cloud::cached_cloud_browser_token(&conn.id) {
        return ProviderCreds::BrowserToken(t.access_token);
    }
    if let Some(secret) = get_cloud_secret(&conn.id, settings) {
        return secret.to_provider_creds();
    }
    resolve_ambient_creds(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::AzureCreds;

    #[test]
    fn defaults_are_off_and_empty() {
        let s = AppSettings::default();
        assert!(!s.cloud_writes_enabled);
        assert!(s.cloud_connections.is_empty());
        assert!(s.cloud_secrets.is_empty());
    }

    #[test]
    fn get_reads_plaintext_fallback_then_delete_clears_it() {
        // A unique id avoids colliding with any real keyring entry.
        let id = "octa-test-conn-9f3a";
        let mut s = AppSettings::default();
        let secret = CloudSecret::AzureKey("k".into());
        s.cloud_secrets
            .insert(id.to_string(), serde_json::to_string(&secret).unwrap());
        assert_eq!(get_cloud_secret(id, &s), Some(secret));
        delete_cloud_secret(id, &mut s);
        assert!(!s.cloud_secrets.contains_key(id));
        assert!(get_cloud_secret(id, &s).is_none());
    }

    #[test]
    fn resolve_creds_falls_back_to_ambient_without_secret() {
        let s = AppSettings::default();
        let conn = CloudConnection::ephemeral_azure("acct", "cont");
        assert!(matches!(
            resolve_creds(&conn, &s),
            ProviderCreds::Azure(AzureCreds::Cli)
        ));
    }

    #[test]
    fn resolve_creds_uses_stored_secret() {
        let mut s = AppSettings::default();
        let conn = CloudConnection::ephemeral_azure("acct", "cont");
        s.cloud_secrets.insert(
            conn.id.clone(),
            serde_json::to_string(&CloudSecret::AzureKey("k".into())).unwrap(),
        );
        assert!(matches!(
            resolve_creds(&conn, &s),
            ProviderCreds::Azure(AzureCreds::AccessKey(_))
        ));
    }
}
