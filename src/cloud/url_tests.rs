use super::*;

#[test]
fn parses_s3_bucket_and_key() {
    let loc = parse_cloud_url("s3://my-bucket/path/to/file.parquet").unwrap();
    assert_eq!(loc.kind, CloudKind::S3);
    assert_eq!(loc.bucket, "my-bucket");
    assert_eq!(loc.key, "path/to/file.parquet");
}

#[test]
fn parses_gcs_and_azure_schemes() {
    assert_eq!(parse_cloud_url("gs://b/k").unwrap().kind, CloudKind::Gcs);
    assert_eq!(
        parse_cloud_url("az://container/blob").unwrap().kind,
        CloudKind::AzureBlob
    );
}

#[test]
fn bucket_root_has_empty_key() {
    let loc = parse_cloud_url("s3://bucket").unwrap();
    assert_eq!(loc.bucket, "bucket");
    assert_eq!(loc.key, "");
    let loc2 = parse_cloud_url("s3://bucket/").unwrap();
    assert_eq!(loc2.bucket, "bucket");
    assert_eq!(loc2.key, "");
}

#[test]
fn keeps_trailing_slash_on_prefix() {
    let loc = parse_cloud_url("s3://bucket/folder/").unwrap();
    assert_eq!(loc.key, "folder/");
}

#[test]
fn rejects_non_cloud_scheme() {
    assert!(parse_cloud_url("https://example.com/x").is_none());
    assert!(parse_cloud_url("/local/path").is_none());
    assert!(parse_cloud_url("file.csv").is_none());
}

#[test]
fn parses_sftp_with_user_and_port() {
    let loc = parse_cloud_url("sftp://anna@files.example.com:2222/home/anna/x.csv").unwrap();
    assert_eq!(loc.kind, CloudKind::Sftp);
    assert_eq!(loc.user.as_deref(), Some("anna"));
    assert_eq!(loc.bucket, "files.example.com:2222");
    assert_eq!(loc.key, "home/anna/x.csv");
}

#[test]
fn the_default_sftp_port_is_dropped_so_a_saved_host_matches() {
    assert_eq!(parse_cloud_url("sftp://h:22/x").unwrap().bucket, "h");
    assert_eq!(split_host_port("h:2222", 22), ("h".to_string(), 2222));
    assert_eq!(split_host_port("h", 22), ("h".to_string(), 22));
    assert_eq!(sftp_authority("h", 22), "h");
    assert_eq!(sftp_authority("h", 2222), "h:2222");
}

#[test]
fn parses_webdav_in_both_spellings() {
    let loc =
        parse_cloud_url("davs://cloud.example.com/remote.php/dav/files/anna/x.parquet").unwrap();
    assert_eq!(loc.kind, CloudKind::WebDav);
    assert_eq!(loc.bucket, "cloud.example.com");
    assert_eq!(loc.key, "remote.php/dav/files/anna/x.parquet");
    assert_eq!(
        parse_cloud_url("dav://nas:8080/d/x.csv").unwrap().kind,
        CloudKind::WebDav
    );
}

#[test]
fn an_at_sign_outside_sftp_stays_in_the_bucket() {
    let loc = parse_cloud_url("s3://a@b/k").unwrap();
    assert_eq!(loc.bucket, "a@b");
    assert!(loc.user.is_none());
}

#[test]
fn rejects_missing_bucket() {
    assert!(parse_cloud_url("s3://").is_none());
    assert!(parse_cloud_url("s3:///key").is_none());
}
