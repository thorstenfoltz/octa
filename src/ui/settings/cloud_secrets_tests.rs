use super::*;
use crate::cloud::AzureCreds;

#[test]
fn defaults_are_off_and_empty() {
    let s = AppSettings::default();
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
