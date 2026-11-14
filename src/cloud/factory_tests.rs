use super::*;

#[test]
fn builds_s3_compatible_via_factory() {
    let mut conn = CloudConnection::ephemeral_s3("bucket");
    conn.endpoint = Some("http://localhost:9000".into());
    conn.force_path_style = true;
    conn.allow_http = true;
    let creds = ProviderCreds::S3(StaticKeys {
        access_key_id: "AKIA".into(),
        secret_access_key: "secret".into(),
        token: None,
    });
    assert!(build_provider(&conn, &creds).is_ok());
}

#[test]
fn builds_azure_via_factory_with_cli() {
    let conn = CloudConnection::ephemeral_azure("acct", "cont");
    assert!(build_provider(&conn, &ProviderCreds::Azure(AzureCreds::Cli)).is_ok());
}

#[test]
fn ambient_s3_compatible_resolves_and_builds() {
    // No profile and (in CI) no AWS_* env -> Ambient, and build still works.
    let mut conn = CloudConnection::ephemeral_s3("bucket");
    conn.endpoint = Some("http://localhost:9000".into());
    conn.allow_http = true;
    let creds = resolve_ambient_creds(&conn);
    // Either Ambient (no env) or S3 (env present); both build.
    assert!(build_provider(&conn, &creds).is_ok());
}

#[test]
fn credentials_that_cannot_be_had_fail_with_their_reason() {
    let conn = CloudConnection::ephemeral_s3("bucket");
    let err = match build_provider(
        &conn,
        &ProviderCreds::Unavailable("install the AWS CLI".into()),
    ) {
        Ok(_) => panic!("a provider was built without credentials"),
        Err(e) => format!("{e:#}"),
    };
    assert!(err.contains("install the AWS CLI"), "{err}");
}
