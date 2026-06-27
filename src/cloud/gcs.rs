//! Google Cloud Storage provider via `object_store`.
//!
//! Credentials, in order: an explicit service-account JSON path (from
//! Settings), else `from_env` (`GOOGLE_*` vars), else the user ADC file written
//! by `gcloud auth application-default login`. All resolved by object_store /
//! discovered here; no extra crate.

use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::gcp::GoogleCloudStorageBuilder;

use super::{CloudConnection, ObjectStoreProvider, credentials::gcs_adc_path};

/// Build a GCS provider. `service_account_path`, when set, wins; otherwise
/// `from_env` plus the discovered ADC file provide credentials.
pub fn build_gcs_provider(
    conn: &CloudConnection,
    service_account_path: Option<&str>,
) -> Result<ObjectStoreProvider> {
    let mut builder = GoogleCloudStorageBuilder::from_env().with_bucket_name(&conn.bucket);
    if conn.anonymous {
        // Public, read-only bucket: skip signing so no credentials are needed.
        builder = builder.with_skip_signature(true);
    } else if let Some(path) = service_account_path {
        builder = builder.with_service_account_path(path);
    } else if let Some(adc) = gcs_adc_path() {
        builder = builder.with_application_credentials(adc.to_string_lossy().to_string());
    }
    let store = builder
        .build()
        .with_context(|| format!("building GCS client for bucket {}", conn.bucket))?;
    Ok(ObjectStoreProvider::new(Arc::new(store)))
}
