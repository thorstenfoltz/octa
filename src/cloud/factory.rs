//! Build a [`CloudProvider`] from a [`CloudConnection`] + resolved credentials.
//!
//! `ProviderCreds` is the typed credential material the caller supplies (the
//! keyring/Settings layer in a later plan produces it; [`resolve_ambient_creds`]
//! produces it from the environment / CLI / ADC for the no-saved-secret case).

use anyhow::Result;

use super::credentials::{AzureCreds, StaticKeys, resolve_s3_keys};
use super::{
    CloudConnection, CloudKind, CloudProvider, build_azure_provider, build_gcs_provider,
    build_s3_provider,
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
        (CloudKind::AzureBlob, ProviderCreds::Azure(az)) => {
            Box::new(build_azure_provider(conn, az)?)
        }
        (CloudKind::AzureBlob, _) => Box::new(build_azure_provider(conn, &AzureCreds::Cli)?),
        (CloudKind::Gcs, ProviderCreds::GcsServiceAccountPath(p)) => {
            Box::new(build_gcs_provider(conn, Some(p))?)
        }
        (CloudKind::Gcs, _) => Box::new(build_gcs_provider(conn, None)?),
    };
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_s3_compatible_via_factory() {
        let mut conn = CloudConnection::ephemeral_s3("bucket");
        conn.endpoint = Some("http://localhost:9000".into());
        conn.force_path_style = true;
        conn.allow_http = true;
        let creds = ProviderCreds::S3(StaticKeys {
            access_key_id: "AKIA".into(),
            secret_access_key: "secret".into(),
            token: None,
        });
        assert!(build_provider(&conn, &creds).is_ok());
    }

    #[test]
    fn builds_azure_via_factory_with_cli() {
        let conn = CloudConnection::ephemeral_azure("acct", "cont");
        assert!(build_provider(&conn, &ProviderCreds::Azure(AzureCreds::Cli)).is_ok());
    }

    #[test]
    fn ambient_s3_compatible_resolves_and_builds() {
        // No profile and (in CI) no AWS_* env -> Ambient, and build still works.
        let mut conn = CloudConnection::ephemeral_s3("bucket");
        conn.endpoint = Some("http://localhost:9000".into());
        conn.allow_http = true;
        let creds = resolve_ambient_creds(&conn);
        // Either Ambient (no env) or S3 (env present); both build.
        assert!(build_provider(&conn, &creds).is_ok());
    }
}
