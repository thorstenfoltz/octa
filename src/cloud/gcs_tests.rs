use super::*;
use crate::cloud::CloudConnection;

fn gcs_conn() -> CloudConnection {
    CloudConnection::ephemeral_gcs("bucket")
}

#[test]
fn builds_gcs_with_bearer_token() {
    assert!(build_gcs_provider_with_token(&gcs_conn(), "tok").is_ok());
}
