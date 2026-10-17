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
