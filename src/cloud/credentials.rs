//! Static credentials, the auth-error type, and per-cloud "how to sign in"
//! hint text. The full credential chain (profiles, cached SSO tokens, managed
//! identity) arrives in a later plan; this plan covers static keys and the
//! `AWS_*` environment.

use std::fmt;
use std::path::{Path, PathBuf};

use super::CloudKind;

/// Static access credentials for an S3 / S3-compatible provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticKeys {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub token: Option<String>,
}

/// Resolve S3 static keys from the standard `AWS_*` environment variables.
pub fn s3_keys_from_env() -> Option<StaticKeys> {
    s3_keys_from(|k| std::env::var(k).ok())
}

/// Testable core: resolve keys from an arbitrary getter.
fn s3_keys_from(get: impl Fn(&str) -> Option<String>) -> Option<StaticKeys> {
    let access_key_id = get("AWS_ACCESS_KEY_ID")?;
    let secret_access_key = get("AWS_SECRET_ACCESS_KEY")?;
    Some(StaticKeys {
        access_key_id,
        secret_access_key,
        token: get("AWS_SESSION_TOKEN"),
    })
}

/// An authentication failure for a cloud, carrying an actionable hint.
#[derive(Debug, Clone)]
pub struct CloudAuthError {
    pub kind: CloudKind,
    pub hint: String,
}

impl fmt::Display for CloudAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not authenticated for {:?}: {}", self.kind, self.hint)
    }
}

impl std::error::Error for CloudAuthError {}

/// The "how to authenticate" hint for a cloud, naming the official CLI command
/// that performs the browser SSO flow.
pub fn auth_hint(kind: CloudKind) -> &'static str {
    match kind {
        CloudKind::S3 => {
            "set AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY, or run: aws sso login --profile <profile>"
        }
        CloudKind::AzureBlob => "run: az login (or set an account key / SAS token in Settings)",
        CloudKind::Gcs => {
            "run: gcloud auth application-default login (or set a service-account JSON path in Settings)"
        }
    }
}

/// The gcloud Application Default Credentials file inside a gcloud config dir.
/// Pure so it can be tested against a temp dir.
pub fn gcs_adc_path_in(gcloud_config_dir: &Path) -> Option<PathBuf> {
    let p = gcloud_config_dir.join("application_default_credentials.json");
    p.exists().then_some(p)
}

/// The user ADC file written by `gcloud auth application-default login`, if it
/// exists. Looks under `$CLOUDSDK_CONFIG` else the platform gcloud config dir.
pub fn gcs_adc_path() -> Option<PathBuf> {
    let dir = if let Ok(c) = std::env::var("CLOUDSDK_CONFIG") {
        PathBuf::from(c)
    } else {
        dirs_gcloud_config_dir()?
    };
    gcs_adc_path_in(&dir)
}

/// Platform gcloud config dir: `%APPDATA%\gcloud` on Windows, else
/// `~/.config/gcloud`.
fn dirs_gcloud_config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("gcloud"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("gcloud"))
    }
}

/// How to authenticate to Azure Blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AzureCreds {
    /// Use the `az login` CLI token (default).
    Cli,
    /// Storage account access key.
    AccessKey(String),
    /// Shared Access Signature token, e.g. `sv=...&sig=...`.
    Sas(String),
}

/// Split a SAS token query string into `(key, value)` pairs for
/// object_store's `with_sas_authorization`.
pub fn parse_sas(token: &str) -> Vec<(String, String)> {
    token
        .trim_start_matches('?')
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            pair.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// Parse the JSON `aws configure export-credentials --format process` prints:
/// `{"Version":1,"AccessKeyId":..,"SecretAccessKey":..,"SessionToken":..}`.
pub fn parse_export_credentials_json(json: &str) -> anyhow::Result<StaticKeys> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let access_key_id = v["AccessKeyId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("export-credentials JSON missing AccessKeyId"))?
        .to_string();
    let secret_access_key = v["SecretAccessKey"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("export-credentials JSON missing SecretAccessKey"))?
        .to_string();
    let token = v["SessionToken"].as_str().map(|s| s.to_string());
    Ok(StaticKeys {
        access_key_id,
        secret_access_key,
        token,
    })
}

/// Resolve AWS credentials for a profile by shelling out to the AWS CLI. This
/// reads the SSO/profile cache populated by `aws sso login`; it is
/// non-interactive (errors if not logged in, never opens a browser).
pub fn aws_export_credentials(profile: &str) -> anyhow::Result<StaticKeys> {
    let out = std::process::Command::new("aws")
        .args([
            "configure",
            "export-credentials",
            "--profile",
            profile,
            "--format",
            "process",
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("running `aws configure export-credentials`: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("aws export-credentials failed: {}", stderr.trim());
    }
    parse_export_credentials_json(&String::from_utf8_lossy(&out.stdout))
}

/// Resolve static S3 keys for a connection: a named profile goes through the
/// AWS CLI (SSO-aware); otherwise the `AWS_*` environment.
pub fn resolve_s3_keys(conn: &super::CloudConnection) -> Option<StaticKeys> {
    if let Some(profile) = &conn.profile {
        return aws_export_credentials(profile).ok();
    }
    s3_keys_from_env()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn resolves_keys_when_env_present() {
        let env: HashMap<&str, &str> = [
            ("AWS_ACCESS_KEY_ID", "AKIA"),
            ("AWS_SECRET_ACCESS_KEY", "secret"),
            ("AWS_SESSION_TOKEN", "tok"),
        ]
        .into_iter()
        .collect();
        let keys = s3_keys_from(|k| env.get(k).map(|s| s.to_string())).unwrap();
        assert_eq!(keys.access_key_id, "AKIA");
        assert_eq!(keys.secret_access_key, "secret");
        assert_eq!(keys.token.as_deref(), Some("tok"));
    }

    #[test]
    fn no_keys_when_secret_missing() {
        let env: HashMap<&str, &str> = [("AWS_ACCESS_KEY_ID", "AKIA")].into_iter().collect();
        assert!(s3_keys_from(|k| env.get(k).map(|s| s.to_string())).is_none());
    }

    #[test]
    fn hints_name_the_official_cli_per_cloud() {
        assert!(auth_hint(CloudKind::S3).contains("aws sso login"));
        assert!(auth_hint(CloudKind::AzureBlob).contains("az login"));
        assert!(auth_hint(CloudKind::Gcs).contains("gcloud auth application-default login"));
    }

    #[test]
    fn parses_export_credentials_json() {
        let json =
            r#"{"Version":1,"AccessKeyId":"AKIA","SecretAccessKey":"sk","SessionToken":"tok"}"#;
        let keys = super::parse_export_credentials_json(json).unwrap();
        assert_eq!(keys.access_key_id, "AKIA");
        assert_eq!(keys.secret_access_key, "sk");
        assert_eq!(keys.token.as_deref(), Some("tok"));
    }

    #[test]
    fn export_credentials_json_without_session_token() {
        let json = r#"{"AccessKeyId":"AKIA","SecretAccessKey":"sk"}"#;
        let keys = super::parse_export_credentials_json(json).unwrap();
        assert!(keys.token.is_none());
    }

    #[test]
    fn export_credentials_json_missing_fields_errors() {
        assert!(super::parse_export_credentials_json(r#"{"Version":1}"#).is_err());
    }

    #[test]
    fn parses_sas_pairs() {
        let pairs = super::parse_sas("?sv=2021-08-06&sig=abc%3D&se=2025");
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], ("sv".to_string(), "2021-08-06".to_string()));
        assert_eq!(pairs[1], ("sig".to_string(), "abc%3D".to_string()));
    }

    #[test]
    fn adc_path_found_only_when_file_present() {
        let dir = tempfile::tempdir().unwrap();
        assert!(super::gcs_adc_path_in(dir.path()).is_none());
        std::fs::write(
            dir.path().join("application_default_credentials.json"),
            "{}",
        )
        .unwrap();
        assert!(super::gcs_adc_path_in(dir.path()).is_some());
    }

    #[test]
    fn auth_error_displays_kind_and_hint() {
        let e = CloudAuthError {
            kind: CloudKind::S3,
            hint: auth_hint(CloudKind::S3).to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("S3"));
        assert!(s.contains("aws sso login"));
    }
}
