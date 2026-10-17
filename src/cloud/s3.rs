//! S3 (and S3-compatible) provider construction via `object_store`.
//!
//! Real AWS (empty endpoint) lets object_store resolve credentials from the
//! environment / instance metadata when no static key is supplied; a custom
//! endpoint (IONOS/MinIO/R2/...) uses path-style addressing and the supplied
//! static key. The full aws-config SSO-cache chain is a later plan.

use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::aws::AmazonS3Builder;

use super::{CloudConnection, ObjectStoreProvider, StaticKeys};

/// Build an [`ObjectStoreProvider`] for an S3 / S3-compatible connection.
/// `keys` are the static credentials to use; `None` falls back to
/// object_store's env/instance resolution (real AWS only).
pub fn build_s3_provider(
    conn: &CloudConnection,
    keys: Option<&StaticKeys>,
) -> Result<ObjectStoreProvider> {
    let mut builder = AmazonS3Builder::from_env().with_bucket_name(&conn.bucket);

    if let Some(region) = &conn.region {
        builder = builder.with_region(region);
    }
    if let Some(endpoint) = &conn.endpoint
        && !endpoint.is_empty()
    {
        builder = builder
            .with_endpoint(endpoint)
            .with_virtual_hosted_style_request(!conn.force_path_style)
            .with_allow_http(conn.allow_http);
    }
    if conn.anonymous {
        // Public, read-only bucket: don't sign requests (and don't fetch any
        // credentials), so access works with no keys and no sign-in.
        builder = builder.with_skip_signature(true);
    } else if let Some(k) = keys {
        builder = builder
            .with_access_key_id(&k.access_key_id)
            .with_secret_access_key(&k.secret_access_key);
        if let Some(token) = &k.token {
            builder = builder.with_token(token);
        }
    }

    let store = builder
        .build()
        .with_context(|| format!("building S3 client for bucket {}", conn.bucket))?;
    Ok(ObjectStoreProvider::new(Arc::new(store)))
}

#[cfg(test)]
#[path = "s3_tests.rs"]
mod tests;
