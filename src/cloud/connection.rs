//! A saved (or ephemeral) cloud connection: the non-secret config needed to
//! build a provider. Secrets (static keys) are passed separately at build
//! time so this struct can be persisted in settings without holding secrets.

use serde::{Deserialize, Serialize};

use super::CloudKind;

/// Connection config for one cloud target. For S3 an empty `endpoint` means
/// real AWS; a set `endpoint` means an S3-compatible provider (IONOS, MinIO,
/// Cloudflare R2, ...), which uses path-style requests and static keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudConnection {
    /// Stable id used to reference this connection (and its keyring secret).
    pub id: String,
    /// Human-readable name shown in the UI.
    pub name: String,
    pub kind: CloudKind,
    /// S3 bucket / Azure container / GCS bucket.
    pub bucket: String,
    /// S3 region (real AWS). Optional for S3-compatible endpoints.
    #[serde(default)]
    pub region: Option<String>,
    /// S3-compatible endpoint URL. Empty/None = real AWS.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Force path-style addressing (`https://host/bucket/key`). Defaults true
    /// for custom endpoints, which most S3-compatibles require.
    #[serde(default)]
    pub force_path_style: bool,
    /// Allow plain HTTP (non-TLS) endpoints, e.g. a local MinIO.
    #[serde(default)]
    pub allow_http: bool,
    /// Keyring reference for this connection's static key, if any.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// Azure storage account name (the container is `bucket`). For GCS this
    /// doubles as the optional gcloud identity (`--account` email) to list
    /// buckets as, when you have several logged-in accounts. Unused for S3.
    #[serde(default)]
    pub account: Option<String>,
    /// AWS named profile to resolve via the CLI (real AWS SSO). None = ambient.
    #[serde(default)]
    pub profile: Option<String>,
    /// Public / anonymous access: skip request signing entirely so a
    /// read-only public bucket/container works without any credentials (and
    /// without redirecting to a sign-in). When set, no secret or sign-in is
    /// needed or used.
    #[serde(default)]
    pub anonymous: bool,
    /// Confine this connection to a key prefix inside `bucket` (e.g.
    /// `team-a/`). None = whole bucket. Stored with a trailing `/`.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Account-level: browse every bucket/container in the account rather than
    /// a single `bucket`. When true, `bucket` may be empty.
    #[serde(default)]
    pub account_level: bool,
    /// GCS project id to scope account-level bucket listing to (buckets in GCP
    /// belong to a project; `gcloud storage buckets list` only sees the active
    /// project otherwise). None = the gcloud active project. Ignored elsewhere.
    #[serde(default)]
    pub project: Option<String>,
}

impl CloudConnection {
    /// Build an ephemeral S3 connection from a bucket name (used when a CLI
    /// URL names a bucket with no saved connection). Real AWS, path-style off,
    /// credentials resolved from the ambient chain at build time.
    pub fn ephemeral_s3(bucket: impl Into<String>) -> Self {
        let bucket = bucket.into();
        Self {
            id: format!("ephemeral-s3-{bucket}"),
            name: bucket.clone(),
            kind: CloudKind::S3,
            bucket,
            region: None,
            endpoint: None,
            force_path_style: false,
            allow_http: false,
            secret_ref: None,
            account: None,
            profile: None,
            anonymous: false,
            prefix: None,
            account_level: false,
            project: None,
        }
    }

    /// Ephemeral Azure connection: needs both the storage `account` and the
    /// `container` (a bare `az://container/...` URL alone cannot name the
    /// account, so callers supply it from env or a saved connection).
    pub fn ephemeral_azure(account: impl Into<String>, container: impl Into<String>) -> Self {
        let account = account.into();
        let container = container.into();
        Self {
            id: format!("ephemeral-az-{account}-{container}"),
            name: format!("{account}/{container}"),
            kind: CloudKind::AzureBlob,
            bucket: container,
            region: None,
            endpoint: None,
            force_path_style: false,
            allow_http: false,
            secret_ref: None,
            account: Some(account),
            profile: None,
            anonymous: false,
            prefix: None,
            account_level: false,
            project: None,
        }
    }

    /// Ephemeral GCS connection from a bucket name.
    pub fn ephemeral_gcs(bucket: impl Into<String>) -> Self {
        let bucket = bucket.into();
        Self {
            id: format!("ephemeral-gs-{bucket}"),
            name: bucket.clone(),
            kind: CloudKind::Gcs,
            bucket,
            region: None,
            endpoint: None,
            force_path_style: false,
            allow_http: false,
            secret_ref: None,
            account: None,
            profile: None,
            anonymous: false,
            prefix: None,
            account_level: false,
            project: None,
        }
    }

    /// Does this connection cover the given parsed URL (kind + bucket + prefix)?
    pub fn covers(&self, loc: &crate::cloud::CloudLocation) -> bool {
        if self.kind != loc.kind {
            return false;
        }
        if self.account_level {
            return true;
        }
        if self.bucket != loc.bucket {
            return false;
        }
        match &self.prefix {
            None => true,
            Some(p) => {
                let p = p.trim_end_matches('/');
                loc.key == p || loc.key.starts_with(&format!("{p}/"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn old_settings_without_new_fields_still_deserialize() {
        let json = r#"{"id":"c1","name":"n","kind":"S3","bucket":"b"}"#;
        let c: CloudConnection = serde_json::from_str(json).unwrap();
        assert!(c.prefix.is_none());
        assert!(!c.account_level);
    }
}
