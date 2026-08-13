//! Resolve a cloud URL to a provider, and download the object it names.
//!
//! One place decides how a bare `s3://` / `az://` / `gs://` URL finds its
//! credentials: a saved connection that `covers` it, else an ephemeral
//! connection on ambient credentials. The CLI cloud actions, the CLI read
//! path and the GUI compare dialog all route through here so they cannot
//! drift apart.
//!
//! The MCP server deliberately keeps its own resolver
//! (`src/mcp/tools/mod.rs::resolve_cloud`): it carries the chat sandbox rule
//! (an unsaved bucket is refused when `restrict_filesystem`), which must not
//! apply to the CLI or the GUI.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cloud::{
    CloudConnection, CloudKind, CloudLocation, CloudProvider, build_provider, parse_cloud_url,
    resolve_ambient_creds,
};
use crate::ui::settings::AppSettings;

/// The saved connection covering `url`, if any. An account-level connection
/// has no fixed bucket, so the returned copy is bound to the bucket the URL
/// names. Pure: no IO and no credential resolution, so this is the part that
/// is unit-testable.
pub fn conn_for_url(url: &str, settings: &AppSettings) -> Option<CloudConnection> {
    let loc = parse_cloud_url(url)?;
    let mut conn = settings
        .cloud_connections
        .iter()
        .find(|c| c.covers(&loc))
        .cloned()?;
    if conn.account_level {
        conn.bucket = loc.bucket.clone();
    }
    Some(conn)
}

/// Resolve `url` to a live provider plus its parsed location. A saved
/// connection wins (its endpoint / region / credentials apply); otherwise an
/// ephemeral connection reads the ambient chain (AWS_* env, cached SSO, Azure
/// CLI login, Google ADC).
pub fn provider_for_url(
    url: &str,
    settings: &AppSettings,
) -> Result<(Box<dyn CloudProvider>, CloudLocation)> {
    let loc = parse_cloud_url(url)
        .with_context(|| format!("not a cloud URL: {url} (expected s3://, az:// or gs://)"))?;
    let (conn, creds) = match conn_for_url(url, settings) {
        Some(conn) => {
            let creds = crate::ui::settings::cloud_secrets::resolve_creds(&conn, settings);
            (conn, creds)
        }
        None => {
            let conn = ephemeral_for(&loc)?;
            let creds = resolve_ambient_creds(&conn);
            (conn, creds)
        }
    };
    Ok((build_provider(&conn, &creds)?, loc))
}

/// Ephemeral connection for a URL no saved connection covers.
fn ephemeral_for(loc: &CloudLocation) -> Result<CloudConnection> {
    Ok(match loc.kind {
        CloudKind::S3 => CloudConnection::ephemeral_s3(&loc.bucket),
        CloudKind::Gcs => CloudConnection::ephemeral_gcs(&loc.bucket),
        CloudKind::AzureBlob => {
            let account = std::env::var("AZURE_STORAGE_ACCOUNT").map_err(|_| {
                anyhow::anyhow!(
                    "Azure needs a storage account: set AZURE_STORAGE_ACCOUNT, or save a \
connection (an az:// URL cannot carry the account name)"
                )
            })?;
            CloudConnection::ephemeral_azure(account, &loc.bucket)
        }
    })
}

/// Download the object `url` names into a temp file and return its path.
///
/// The temp keeps the object's extension so the format registry routes it to
/// the right reader, and the handle is leaked (`tmp.keep()`) so streaming
/// readers can keep reading from disk after this returns. The OS clears the
/// temp directory, the same trick the archive viewer uses.
pub fn fetch_url_to_temp(url: &str, settings: &AppSettings) -> Result<PathBuf> {
    let (provider, loc) = provider_for_url(url, settings)?;
    let bytes = provider
        .get(&loc.key)
        .with_context(|| format!("downloading {url}"))?;
    let ext = std::path::Path::new(&loc.key)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let tmp = tempfile::Builder::new()
        .prefix("octa-cloud-")
        .suffix(&format!(".{ext}"))
        .tempfile()?;
    tmp.as_file().write_all(&bytes)?;
    let path = tmp.path().to_path_buf();
    let _ = tmp.keep();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudConnection;
    use crate::ui::settings::AppSettings;

    /// `AppSettings::default()`, never `::load()`: a test must not read the
    /// developer's real settings.toml.
    fn settings_with(conns: Vec<CloudConnection>) -> AppSettings {
        AppSettings {
            cloud_connections: conns,
            ..Default::default()
        }
    }

    #[test]
    fn saved_connection_covering_the_url_wins() {
        let mut c = CloudConnection::ephemeral_s3("data");
        c.name = "saved".to_string();
        let s = settings_with(vec![c]);
        assert_eq!(conn_for_url("s3://data/x.csv", &s).unwrap().name, "saved");
        assert!(conn_for_url("s3://other/x.csv", &s).is_none());
    }

    #[test]
    fn account_level_connection_binds_the_urls_bucket() {
        let mut c = CloudConnection::ephemeral_s3("");
        c.account_level = true;
        let s = settings_with(vec![c]);
        let got = conn_for_url("s3://any-bucket/x.csv", &s).unwrap();
        assert_eq!(got.bucket, "any-bucket");
    }

    #[test]
    fn a_prefix_scoped_connection_only_covers_its_prefix() {
        let mut c = CloudConnection::ephemeral_s3("data");
        c.prefix = Some("team-a".to_string());
        let s = settings_with(vec![c]);
        assert!(conn_for_url("s3://data/team-a/x.csv", &s).is_some());
        assert!(conn_for_url("s3://data/team-b/x.csv", &s).is_none());
    }

    #[test]
    fn a_local_path_is_not_a_cloud_url() {
        assert!(conn_for_url("/tmp/x.csv", &AppSettings::default()).is_none());
    }
}
