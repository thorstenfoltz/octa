//! Stored credential material for a saved cloud connection: the value kept in
//! the keyring (or the settings plaintext fallback), plus its mapping to the
//! runtime [`ProviderCreds`]. GCS uses ambient ADC / `GOOGLE_*` env rather than
//! a stored secret, so it has no variant here.

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
mod tests {
    use super::*;

    #[test]
    fn s3_secret_roundtrips_through_json() {
        let s = CloudSecret::S3 {
            access_key_id: "AKIA".into(),
            secret_access_key: "sk".into(),
            token: Some("tok".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: CloudSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn azure_variants_roundtrip() {
        for s in [
            CloudSecret::AzureKey("k".into()),
            CloudSecret::AzureSas("sv=1".into()),
        ] {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(serde_json::from_str::<CloudSecret>(&json).unwrap(), s);
        }
    }

    #[test]
    fn maps_to_provider_creds() {
        let s3 = CloudSecret::S3 {
            access_key_id: "AKIA".into(),
            secret_access_key: "sk".into(),
            token: None,
        };
        assert!(matches!(s3.to_provider_creds(), ProviderCreds::S3(_)));
        assert!(matches!(
            CloudSecret::AzureKey("k".into()).to_provider_creds(),
            ProviderCreds::Azure(AzureCreds::AccessKey(_))
        ));
        assert!(matches!(
            CloudSecret::AzureSas("sv=1".into()).to_provider_creds(),
            ProviderCreds::Azure(AzureCreds::Sas(_))
        ));
    }
}
