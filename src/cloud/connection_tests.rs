use super::*;

#[test]
fn ephemeral_s3_defaults_to_real_aws() {
    let c = CloudConnection::ephemeral_s3("my-bucket");
    assert_eq!(c.kind, CloudKind::S3);
    assert_eq!(c.bucket, "my-bucket");
    assert!(c.endpoint.is_none());
    assert!(!c.force_path_style);
}

#[test]
fn connection_roundtrips_through_json() {
    let c = CloudConnection {
        id: "c1".into(),
        name: "IONOS prod".into(),
        kind: CloudKind::S3,
        bucket: "data".into(),
        region: Some("de".into()),
        endpoint: Some("https://s3.eu-central-1.ionoscloud.com".into()),
        force_path_style: true,
        allow_http: false,
        secret_ref: Some("cloud:c1".into()),
        account: None,
        profile: None,
        anonymous: false,
        prefix: None,
        account_level: false,
        project: None,
        allow_writes: false,
        oauth_client_id: None,
        oauth_tenant: None,
        ..Default::default()
    };
    let json = serde_json::to_string(&c).unwrap();
    let back: CloudConnection = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

#[test]
fn ephemeral_azure_carries_account_and_container() {
    let c = CloudConnection::ephemeral_azure("acct", "cont");
    assert_eq!(c.kind, CloudKind::AzureBlob);
    assert_eq!(c.account.as_deref(), Some("acct"));
    assert_eq!(c.bucket, "cont");
}

#[test]
fn covers_respects_prefix_and_account_level() {
    use crate::cloud::parse_cloud_url;
    let mut c = CloudConnection::ephemeral_s3("data");
    // Plain bucket scope: covers its bucket, not another.
    assert!(c.covers(&parse_cloud_url("s3://data/x.csv").unwrap()));
    assert!(!c.covers(&parse_cloud_url("s3://other/x.csv").unwrap()));

    // Prefix scope: only keys under the prefix.
    c.prefix = Some("team-a/".into());
    assert!(c.covers(&parse_cloud_url("s3://data/team-a/x.csv").unwrap()));
    assert!(c.covers(&parse_cloud_url("s3://data/team-a/").unwrap()));
    assert!(!c.covers(&parse_cloud_url("s3://data/team-b/x.csv").unwrap()));

    // Account scope: any bucket of the same kind.
    let mut a = CloudConnection::ephemeral_s3("");
    a.account_level = true;
    assert!(a.covers(&parse_cloud_url("s3://anything/x.csv").unwrap()));
    assert!(!a.covers(&parse_cloud_url("gs://anything/x.csv").unwrap()));
}

#[test]
fn an_sftp_url_naming_another_user_is_not_covered() {
    use crate::cloud::parse_cloud_url;
    let mut c = CloudConnection::ephemeral_sftp("files.example.com", Some("anna".into()));
    assert!(c.covers(&parse_cloud_url("sftp://files.example.com/x.csv").unwrap()));
    assert!(c.covers(&parse_cloud_url("sftp://anna@files.example.com/x.csv").unwrap()));
    assert!(!c.covers(&parse_cloud_url("sftp://ben@files.example.com/x.csv").unwrap()));
    c.bucket = "files.example.com:2222".into();
    assert_eq!(c.sftp_host_port(), ("files.example.com".to_string(), 2222));
}

#[test]
fn a_webdav_connection_covers_its_server_folder() {
    use crate::cloud::parse_cloud_url;
    let c = CloudConnection {
        kind: CloudKind::WebDav,
        bucket: "cloud.example.com".into(),
        prefix: Some("remote.php/dav/files/anna/".into()),
        endpoint: Some("https://cloud.example.com/remote.php/dav/files/anna/".into()),
        ..Default::default()
    };
    assert!(c.covers(
        &parse_cloud_url("davs://cloud.example.com/remote.php/dav/files/anna/a.csv").unwrap()
    ));
    assert!(!c.covers(
        &parse_cloud_url("davs://cloud.example.com/remote.php/dav/files/ben/a.csv").unwrap()
    ));
}

#[test]
fn old_settings_without_new_fields_still_deserialize() {
    let json = r#"{"id":"c1","name":"n","kind":"S3","bucket":"b"}"#;
    let c: CloudConnection = serde_json::from_str(json).unwrap();
    assert!(c.prefix.is_none());
    assert!(!c.account_level);
    // Secure by default: a connection saved before the field existed is
    // read-only until the user opts it in.
    assert!(!c.allow_writes);
    assert!(c.username.is_none());
    assert!(c.ssh_auth.is_none());
    assert!(!c.accept_new_host_key);
}
