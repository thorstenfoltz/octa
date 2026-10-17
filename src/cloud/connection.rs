//! A saved (or ephemeral) cloud connection: the non-secret config needed to
//! build a provider. Secrets (static keys) are passed separately at build
//! time so this struct can be persisted in settings without holding secrets.

use serde::{Deserialize, Serialize};

use super::CloudKind;

/// Connection config for one cloud target. For S3 an empty `endpoint` means
/// real AWS; a set `endpoint` means an S3-compatible provider (IONOS, MinIO,
/// Cloudflare R2, ...), which uses path-style requests and static keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Write permission for this connection: save-back, upload, and the
    /// copy / move / delete operations. The **only** switch governing cloud
    /// writes; there is deliberately no global one on top of it. Default
    /// false, so every connection is read-only until the user opts it in
    /// (existing saved connections need the tick after upgrading).
    #[serde(default)]
    pub allow_writes: bool,
    /// BYO OAuth client id for native browser sign-in (Azure Blob / GCS
    /// fallback when the vendor CLI is unavailable). None = not configured.
    #[serde(default)]
    pub oauth_client_id: Option<String>,
    /// Azure tenant for the browser endpoints (Azure only). None = "organizations".
    #[serde(default)]
    pub oauth_tenant: Option<String>,
    /// The account on an SFTP or WebDAV server. Unused for object stores.
    #[serde(default)]
    pub username: Option<String>,
    /// How an SFTP connection signs in (agent, key file, password). The
    /// password or key passphrase lives in the keyring like every other
    /// cloud secret. `None` = agent.
    #[serde(default)]
    pub ssh_auth: Option<crate::ssh::SshAuth>,
    /// Accept and remember an SFTP server's key the first time it is seen
    /// (OpenSSH's `accept-new`). A changed key is refused regardless.
    #[serde(default)]
    pub accept_new_host_key: bool,
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
            ..Default::default()
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
            account: Some(account),
            ..Default::default()
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
            ..Default::default()
        }
    }

    /// Ephemeral SFTP connection for an `sftp://` URL no saved connection
    /// covers: the SSH agent, the URL's user (else the OS user), strict
    /// host-key checking.
    pub fn ephemeral_sftp(authority: impl Into<String>, user: Option<String>) -> Self {
        let authority = authority.into();
        let user = user.or_else(|| {
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .ok()
        });
        Self {
            id: format!("ephemeral-sftp-{authority}"),
            name: authority.clone(),
            kind: CloudKind::Sftp,
            bucket: authority,
            username: user,
            ssh_auth: Some(crate::ssh::SshAuth::Agent),
            ..Default::default()
        }
    }

    /// Host and port of an SFTP connection, read from `bucket`.
    pub fn sftp_host_port(&self) -> (String, u16) {
        crate::cloud::split_host_port(&self.bucket, 22)
    }

    /// Does this connection cover the given parsed URL (kind + bucket + prefix)?
    pub fn covers(&self, loc: &crate::cloud::CloudLocation) -> bool {
        if self.kind != loc.kind {
            return false;
        }
        if self.kind == CloudKind::Sftp
            && let Some(user) = &loc.user
            && self.username.as_deref() != Some(user.as_str())
        {
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
#[path = "connection_tests.rs"]
mod tests;
