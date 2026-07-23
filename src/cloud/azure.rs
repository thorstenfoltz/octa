//! Azure Blob Storage provider via `object_store`.
//!
//! Credentials come from [`AzureCreds`]: the `az login` CLI token (default,
//! via `with_use_azure_cli`), a storage account access key, or a SAS token.
//! Managed identity / env are picked up by `from_env`.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use object_store::azure::MicrosoftAzureBuilder;

use super::credentials::{AzureCreds, parse_sas};
use super::{CloudConnection, ObjectStoreProvider};

/// Build an Azure Blob provider. `conn.account` (storage account) is required;
/// `conn.bucket` is the container.
pub fn build_azure_provider(
    conn: &CloudConnection,
    creds: &AzureCreds,
) -> Result<ObjectStoreProvider> {
    let Some(account) = &conn.account else {
        bail!(
            "Azure connection {} is missing the storage account name",
            conn.name
        );
    };
    let mut builder = MicrosoftAzureBuilder::from_env()
        .with_account(account)
        .with_container_name(&conn.bucket)
        .with_allow_http(conn.allow_http);
    builder = if conn.anonymous {
        // Public, read-only container: skip signing so an anonymous blob is
        // read directly instead of redirecting to a sign-in.
        builder.with_skip_signature(true)
    } else {
        match creds {
            AzureCreds::Cli => builder.with_use_azure_cli(true),
            AzureCreds::AccessKey(k) => builder.with_access_key(k),
            AzureCreds::Sas(s) => builder.with_sas_authorization(parse_sas(s)),
            AzureCreds::BearerToken(t) => builder.with_bearer_token_authorization(t.clone()),
        }
    };
    let store = builder
        .build()
        .with_context(|| format!("building Azure client for {}/{}", account, conn.bucket))?;
    Ok(ObjectStoreProvider::new(Arc::new(store)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudKind;

    fn azure_conn() -> CloudConnection {
        let mut c = CloudConnection::ephemeral_azure("acct", "cont");
        c.kind = CloudKind::AzureBlob;
        c
    }

    #[test]
    fn builds_with_cli_creds_without_network() {
        // `with_use_azure_cli` does not invoke `az` at build time, so this is
        // hermetic.
        let provider = build_azure_provider(&azure_conn(), &AzureCreds::Cli);
        assert!(provider.is_ok());
    }

    #[test]
    fn builds_with_bearer_token() {
        let provider = build_azure_provider(&azure_conn(), &AzureCreds::BearerToken("tok".into()));
        assert!(provider.is_ok());
    }

    #[test]
    fn builds_with_access_key() {
        let provider = build_azure_provider(&azure_conn(), &AzureCreds::AccessKey("a2V5".into()));
        assert!(provider.is_ok());
    }

    #[test]
    fn missing_account_is_an_error() {
        let mut c = azure_conn();
        c.account = None;
        assert!(build_azure_provider(&c, &AzureCreds::Cli).is_err());
    }

    #[test]
    fn builds_anonymous_public_container_without_creds() {
        // A public, read-only container skips signing; creds are ignored.
        let mut c = azure_conn();
        c.anonymous = true;
        assert!(build_azure_provider(&c, &AzureCreds::Cli).is_ok());
    }
}
