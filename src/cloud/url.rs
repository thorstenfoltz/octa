//! Parsing of cloud object-storage URLs into (kind, bucket, key).
//!
//! Supported schemes: `s3://`, `az://`, `gs://`, `sftp://`, `davs://` (and
//! `dav://`). A URL is `scheme://<bucket>/<key...>`, where `bucket` is the
//! first path segment (the S3 bucket, Azure container, GCS bucket, or a file
//! server's `host[:port]`) and `key` is the rest (possibly empty for a bucket
//! root). Trailing slashes on the key are kept as-is so a directory prefix
//! round-trips.

use serde::{Deserialize, Serialize};

/// Which cloud a URL or connection targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CloudKind {
    #[default]
    S3,
    AzureBlob,
    Gcs,
    /// An SFTP server. `bucket` holds `host` or `host:port`.
    Sftp,
    /// A WebDAV server (Nextcloud, ownCloud, NAS boxes). `bucket` holds
    /// `host[:port]`, the connection's `endpoint` the full address.
    WebDav,
}

impl CloudKind {
    /// The URL scheme (without `://`) for this cloud.
    pub fn scheme(self) -> &'static str {
        match self {
            CloudKind::S3 => "s3",
            CloudKind::AzureBlob => "az",
            CloudKind::Gcs => "gs",
            CloudKind::Sftp => "sftp",
            CloudKind::WebDav => "davs",
        }
    }

    /// A file server (SFTP, WebDAV) rather than an object store: no buckets
    /// to list, no vendor CLI to sign in with, no anonymous mode.
    pub fn is_file_server(self) -> bool {
        matches!(self, CloudKind::Sftp | CloudKind::WebDav)
    }
}

/// A parsed cloud URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudLocation {
    pub kind: CloudKind,
    /// First path segment: S3 bucket / Azure container / GCS bucket.
    pub bucket: String,
    /// Remaining path after the bucket (no leading slash). May be empty.
    pub key: String,
    /// The user named in `sftp://user@host/...`. `None` for every other kind.
    pub user: Option<String>,
}

/// Parse `scheme://bucket/key...` into a [`CloudLocation`]. Returns `None`
/// for a non-cloud scheme or a URL with no bucket segment.
pub fn parse_cloud_url(input: &str) -> Option<CloudLocation> {
    let (scheme, rest) = input.split_once("://")?;
    let kind = match scheme {
        "s3" => CloudKind::S3,
        "az" => CloudKind::AzureBlob,
        "gs" => CloudKind::Gcs,
        "sftp" => CloudKind::Sftp,
        "davs" | "dav" => CloudKind::WebDav,
        _ => return None,
    };
    // `rest` is `bucket` or `bucket/key...`. The bucket is everything up to
    // the first `/`; the key is the remainder (which keeps any trailing `/`).
    let (authority, key) = match rest.split_once('/') {
        Some((b, k)) => (b, k),
        None => (rest, ""),
    };
    // `user@` means something to SFTP only; anywhere else it stays part of
    // the bucket name, as it always did.
    let (user, mut bucket) = match (kind, authority.rsplit_once('@')) {
        (CloudKind::Sftp, Some((u, host))) if !u.is_empty() => {
            (Some(u.to_string()), host.to_string())
        }
        _ => (None, authority.to_string()),
    };
    if kind == CloudKind::Sftp {
        // `host:22` and `host` are one server; saved connections store the
        // short form.
        let (host, port) = split_host_port(&bucket, 22);
        bucket = sftp_authority(&host, port);
    }
    if bucket.is_empty() {
        return None;
    }
    Some(CloudLocation {
        kind,
        bucket,
        key: key.to_string(),
        user,
    })
}

/// `host:port` -> (`host`, port); no port, or one that does not parse, gives
/// `default_port`.
pub fn split_host_port(authority: &str, default_port: u16) -> (String, u16) {
    match authority.rsplit_once(':') {
        Some((host, port)) => match port.parse() {
            Ok(p) => (host.to_string(), p),
            Err(_) => (authority.to_string(), default_port),
        },
        None => (authority.to_string(), default_port),
    }
}

/// The `bucket` an SFTP connection stores: the host alone on port 22.
pub fn sftp_authority(host: &str, port: u16) -> String {
    let host = host.trim();
    if port == 22 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
#[path = "url_tests.rs"]
mod tests;
