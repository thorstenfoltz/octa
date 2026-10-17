//! Stored credential material for a saved cloud connection: the value kept in
//! the keyring (or the settings plaintext fallback), plus its mapping to the
//! runtime [`ProviderCreds`]. GCS uses ambient ADC / `GOOGLE_*` env rather than
//! a stored secret, so it has no variant here; SFTP and WebDAV share
//! [`CloudSecret::Password`].

use serde::{Deserialize, Serialize};

use super::{AzureCreds, ProviderCreds, StaticKeys};

/// A secret stored for a saved connection. Serialised to JSON for the keyring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudSecret {
    /// S3 / S3-compatible static access key (+ optional session token).
    S3 {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default)]
        token: Option<String>,
    },
    /// Azure storage account access key.
    AzureKey(String),
    /// Azure SAS token.
    AzureSas(String),
    /// Google OAuth client secret for native browser sign-in (GCS). Not a
    /// provider credential by itself: the browser flow exchanges it for a
    /// bearer token.
    GcsOAuthClientSecret(String),
    /// SFTP password or key passphrase, or a WebDAV (app) password.
    Password(String),
}

impl CloudSecret {
    /// Convert the stored secret into runtime provider credentials.
    pub fn to_provider_creds(&self) -> ProviderCreds {
        match self {
            CloudSecret::S3 {
                access_key_id,
                secret_access_key,
                token,
            } => ProviderCreds::S3(StaticKeys {
                access_key_id: access_key_id.clone(),
                secret_access_key: secret_access_key.clone(),
                token: token.clone(),
            }),
            CloudSecret::AzureKey(k) => ProviderCreds::Azure(AzureCreds::AccessKey(k.clone())),
            CloudSecret::AzureSas(s) => ProviderCreds::Azure(AzureCreds::Sas(s.clone())),
            // The client secret alone cannot authenticate; the browser flow
            // yields the real bearer token. Fall back to ambient otherwise.
            CloudSecret::GcsOAuthClientSecret(_) => ProviderCreds::Ambient,
            CloudSecret::Password(p) => ProviderCreds::Password(p.clone()),
        }
    }

    /// The Google OAuth client secret string, if this is that variant.
    pub fn oauth_client_secret(&self) -> Option<&str> {
        match self {
            CloudSecret::GcsOAuthClientSecret(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
