use super::*;
use std::collections::HashMap;

#[test]
fn cloud_browser_config_azure_storage_scope() {
    let mut c = CloudConnection::ephemeral_azure("acct", "cont");
    c.oauth_client_id = Some("cid".into());
    c.oauth_tenant = Some("contoso".into());
    let cfg = cloud_browser_oauth_config(&c, None).expect("azure cfg");
    assert_eq!(cfg.scope, "https://storage.azure.com/.default");
    assert!(cfg.authorize_url.contains("contoso"));
    assert!(cfg.client_secret.is_none());
}

#[test]
fn cloud_browser_config_gcs_scope_and_secret() {
    let mut c = CloudConnection::ephemeral_gcs("bucket");
    c.oauth_client_id = Some("cid".into());
    let cfg = cloud_browser_oauth_config(&c, Some("gsecret")).expect("gcs cfg");
    assert_eq!(cfg.scope, "https://www.googleapis.com/auth/cloud-platform");
    assert_eq!(cfg.token_url, "https://oauth2.googleapis.com/token");
    assert_eq!(cfg.client_secret.as_deref(), Some("gsecret"));
}

#[test]
fn cloud_browser_config_none_for_s3_or_no_client() {
    let mut s3 = CloudConnection::ephemeral_s3("b");
    s3.oauth_client_id = Some("cid".into());
    assert!(cloud_browser_oauth_config(&s3, None).is_none());
    let gcs = CloudConnection::ephemeral_gcs("bucket");
    assert!(cloud_browser_oauth_config(&gcs, None).is_none());
}

#[test]
fn resolves_keys_when_env_present() {
    let env: HashMap<&str, &str> = [
        ("AWS_ACCESS_KEY_ID", "AKIA"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("AWS_SESSION_TOKEN", "tok"),
    ]
    .into_iter()
    .collect();
    let keys = s3_keys_from(|k| env.get(k).map(|s| s.to_string())).unwrap();
    assert_eq!(keys.access_key_id, "AKIA");
    assert_eq!(keys.secret_access_key, "secret");
    assert_eq!(keys.token.as_deref(), Some("tok"));
}

#[test]
fn no_keys_when_secret_missing() {
    let env: HashMap<&str, &str> = [("AWS_ACCESS_KEY_ID", "AKIA")].into_iter().collect();
    assert!(s3_keys_from(|k| env.get(k).map(|s| s.to_string())).is_none());
}

#[test]
fn hints_name_the_official_cli_per_cloud() {
    assert!(auth_hint(CloudKind::S3).contains("aws sso login"));
    assert!(auth_hint(CloudKind::AzureBlob).contains("az login"));
    assert!(auth_hint(CloudKind::Gcs).contains("gcloud auth application-default login"));
}

#[test]
fn parses_export_credentials_json() {
    let json = r#"{"Version":1,"AccessKeyId":"AKIA","SecretAccessKey":"sk","SessionToken":"tok"}"#;
    let keys = super::parse_export_credentials_json(json).unwrap();
    assert_eq!(keys.access_key_id, "AKIA");
    assert_eq!(keys.secret_access_key, "sk");
    assert_eq!(keys.token.as_deref(), Some("tok"));
}

#[test]
fn export_credentials_json_without_session_token() {
    let json = r#"{"AccessKeyId":"AKIA","SecretAccessKey":"sk"}"#;
    let keys = super::parse_export_credentials_json(json).unwrap();
    assert!(keys.token.is_none());
}

#[test]
fn export_credentials_json_missing_fields_errors() {
    assert!(super::parse_export_credentials_json(r#"{"Version":1}"#).is_err());
}

#[test]
fn parses_sas_pairs() {
    let pairs = super::parse_sas("?sv=2021-08-06&sig=abc%3D&se=2025");
    assert_eq!(pairs.len(), 3);
    assert_eq!(pairs[0], ("sv".to_string(), "2021-08-06".to_string()));
    assert_eq!(pairs[1], ("sig".to_string(), "abc%3D".to_string()));
}

#[test]
fn adc_path_found_only_when_file_present() {
    let dir = tempfile::tempdir().unwrap();
    assert!(super::gcs_adc_path_in(dir.path()).is_none());
    std::fs::write(
        dir.path().join("application_default_credentials.json"),
        "{}",
    )
    .unwrap();
    assert!(super::gcs_adc_path_in(dir.path()).is_some());
}

#[test]
fn auth_error_displays_kind_and_hint() {
    let e = CloudAuthError {
        kind: CloudKind::S3,
        hint: auth_hint(CloudKind::S3).to_string(),
    };
    let s = e.to_string();
    assert!(s.contains("S3"));
    assert!(s.contains("aws sso login"));
}
