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
#[path = "azure_tests.rs"]
mod tests;
