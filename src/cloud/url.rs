//! Parsing of cloud object-storage URLs into (kind, bucket, key).
//!
//! Supported schemes: `s3://`, `az://`, `gs://`. A URL is
//! `scheme://<bucket>/<key...>`, where `bucket` is the first path segment
//! (the S3 bucket, Azure container, or GCS bucket) and `key` is the rest
//! (possibly empty for a bucket root). Trailing slashes on the key are kept
//! as-is so a directory prefix round-trips.

use serde::{Deserialize, Serialize};

/// Which cloud a URL or connection targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CloudKind {
    #[default]
    S3,
    AzureBlob,
    Gcs,
}

impl CloudKind {
    /// The URL scheme (without `://`) for this cloud.
    pub fn scheme(self) -> &'static str {
        match self {
            CloudKind::S3 => "s3",
            CloudKind::AzureBlob => "az",
            CloudKind::Gcs => "gs",
        }
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
}

/// Parse `scheme://bucket/key...` into a [`CloudLocation`]. Returns `None`
/// for a non-cloud scheme or a URL with no bucket segment.
pub fn parse_cloud_url(input: &str) -> Option<CloudLocation> {
    let (scheme, rest) = input.split_once("://")?;
    let kind = match scheme {
        "s3" => CloudKind::S3,
        "az" => CloudKind::AzureBlob,
        "gs" => CloudKind::Gcs,
        _ => return None,
    };
    // `rest` is `bucket` or `bucket/key...`. The bucket is everything up to
    // the first `/`; the key is the remainder (which keeps any trailing `/`).
    let (bucket, key) = match rest.split_once('/') {
        Some((b, k)) => (b, k),
        None => (rest, ""),
    };
    if bucket.is_empty() {
        return None;
    }
    Some(CloudLocation {
        kind,
        bucket: bucket.to_string(),
        key: key.to_string(),
    })
}

#[cfg(test)]
mod tests {
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
    fn rejects_missing_bucket() {
        assert!(parse_cloud_url("s3://").is_none());
        assert!(parse_cloud_url("s3:///key").is_none());
    }
}
