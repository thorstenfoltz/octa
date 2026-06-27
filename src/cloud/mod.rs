//! Cloud object storage: connect to S3 (and S3-compatible providers such as
//! IONOS, MinIO, Cloudflare R2), Azure Blob, and Google Cloud Storage, browse
//! folders/files, and open files by downloading them to a temp file.
//!
//! `object_store` provides one async trait over every backend; octa is sync,
//! so [`ObjectStoreProvider`] wraps an `Arc<dyn ObjectStore>` and runs each
//! call to completion on a shared, lazily-built tokio runtime ([`runtime`]).
//!
//! This module is built incrementally (one cloud per file, mirroring
//! `src/formats/mod.rs`); see the per-cloud files for the backend specifics.

use std::sync::OnceLock;

mod url;
pub use url::{CloudKind, CloudLocation, parse_cloud_url};

mod credentials;
pub use credentials::{
    AzureCreds, CloudAuthError, StaticKeys, auth_hint, aws_export_credentials, gcs_adc_path,
    parse_export_credentials_json, parse_sas, resolve_s3_keys, s3_keys_from_env,
};

mod connection;
pub use connection::CloudConnection;

mod provider;
pub use provider::{CloudProvider, ObjectEntry, ObjectStoreProvider};

mod s3;
pub use s3::build_s3_provider;

mod gcs;
pub use gcs::build_gcs_provider;

mod azure;
pub use azure::build_azure_provider;

mod login;
pub use login::{cli_available, cli_binary, interactive_login, login_command};

mod factory;
pub use factory::{ProviderCreds, build_provider, resolve_ambient_creds};

mod secret;
pub use secret::CloudSecret;

/// Shared multi-thread tokio runtime for blocking on `object_store` futures.
/// Built on first use; the GUI/CLI never construct it unless a cloud call runs.
pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("building the cloud tokio runtime")
    })
}
