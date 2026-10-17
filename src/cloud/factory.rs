//! Build a [`CloudProvider`] from a [`CloudConnection`] + resolved credentials.
//!
//! `ProviderCreds` is the typed credential material the caller supplies (the
//! keyring/Settings layer in a later plan produces it; [`resolve_ambient_creds`]
//! produces it from the environment / CLI / ADC for the no-saved-secret case).

use anyhow::Result;

use super::credentials::{AzureCreds, StaticKeys, resolve_s3_keys};
use super::{
    CloudConnection, CloudKind, CloudProvider, build_azure_provider, build_gcs_provider,
    build_gcs_provider_with_token, build_s3_provider, build_sftp_provider,
    build_webdav_provider_for,
};

/// Resolved credential material for a provider, per cloud.
#[derive(Debug, Clone)]
pub enum ProviderCreds {
    /// Resolve from the ambient environment / CLI / ADC (no stored secret).
    Ambient,
    /// S3 static keys.
    S3(StaticKeys),
    /// Azure auth choice.
    Azure(AzureCreds),
    /// GCS service-account JSON file path.
    GcsServiceAccountPath(String),
    /// A raw OAuth bearer access token from native browser sign-in (Azure /
    /// GCS). Session-only; the caller re-mints on expiry.
    BrowserToken(String),
    /// SFTP password / key passphrase or WebDAV password.
    Password(String),
}

/// Resolve ambient credentials for a connection (no stored secret).
pub fn resolve_ambient_creds(conn: &CloudConnection) -> ProviderCreds {
    match conn.kind {
        CloudKind::S3 => match resolve_s3_keys(conn) {
            Some(keys) => ProviderCreds::S3(keys),
            None => ProviderCreds::Ambient,
        },
        // `az login` token via the CLI is the ambient default for Azure.
        CloudKind::AzureBlob => ProviderCreds::Azure(AzureCreds::Cli),
        // GCS resolves ADC / env inside the builder.
        CloudKind::Gcs => ProviderCreds::Ambient,
        // SFTP's agent and key file are read by the SSH sign-in itself;
        // WebDAV without a saved password goes anonymous.
        CloudKind::Sftp | CloudKind::WebDav => ProviderCreds::Ambient,
    }
}

/// Build a provider for a connection with the given credentials.
pub fn build_provider(
    conn: &CloudConnection,
    creds: &ProviderCreds,
) -> Result<Box<dyn CloudProvider>> {
    let provider: Box<dyn CloudProvider> = match (conn.kind, creds) {
        (CloudKind::S3, ProviderCreds::S3(keys)) => Box::new(build_s3_provider(conn, Some(keys))?),
        (CloudKind::S3, _) => Box::new(build_s3_provider(conn, None)?),
        (CloudKind::AzureBlob, ProviderCreds::BrowserToken(t)) => Box::new(build_azure_provider(
            conn,
            &AzureCreds::BearerToken(t.clone()),
        )?),
        (CloudKind::AzureBlob, ProviderCreds::Azure(az)) => {
            Box::new(build_azure_provider(conn, az)?)
        }
        (CloudKind::AzureBlob, _) => Box::new(build_azure_provider(conn, &AzureCreds::Cli)?),
        (CloudKind::Gcs, ProviderCreds::BrowserToken(t)) => {
            Box::new(build_gcs_provider_with_token(conn, t)?)
        }
        (CloudKind::Gcs, ProviderCreds::GcsServiceAccountPath(p)) => {
            Box::new(build_gcs_provider(conn, Some(p))?)
        }
        (CloudKind::Gcs, _) => Box::new(build_gcs_provider(conn, None)?),
        (CloudKind::Sftp, _) => Box::new(build_sftp_provider(conn, creds)),
        (CloudKind::WebDav, _) => Box::new(build_webdav_provider_for(conn, creds)?),
    };
    Ok(provider)
}

#[cfg(test)]
#[path = "factory_tests.rs"]
mod tests;
