use super::*;
use crate::cloud::CloudKind;

fn s3_compatible_conn() -> CloudConnection {
    CloudConnection {
        id: "t".into(),
        name: "MinIO".into(),
        kind: CloudKind::S3,
        bucket: "bucket".into(),
        region: Some("us-east-1".into()),
        endpoint: Some("http://localhost:9000".into()),
        force_path_style: true,
        allow_http: true,
        allow_writes: true,
        ..Default::default()
    }
}

#[test]
fn builds_s3_compatible_provider_without_network() {
    // build() configures the client; it does not connect, so this is
    // hermetic. We only assert construction succeeds.
    let keys = StaticKeys {
        access_key_id: "AKIA".into(),
        secret_access_key: "secret".into(),
        token: None,
    };
    let provider = build_s3_provider(&s3_compatible_conn(), Some(&keys));
    assert!(provider.is_ok());
}
