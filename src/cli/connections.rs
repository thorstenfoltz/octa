//! `--add-connection` / `--remove-connection`: manage saved cloud and database
//! connections without the Settings dialog.
//!
//! Built for headless use (a container, a CI job, a provisioning script), which
//! shapes three decisions:
//!
//! - **One spec flag, not a flag per field.** A cloud connection has fourteen
//!   fields and a database one more; `--add-connection 'kind=s3,name=prod,...'`
//!   keeps the surface to one flag and lets unknown-to-this-version keys be
//!   reported rather than silently ignored.
//! - **Secrets never in argv.** `--secret-env VAR` names an environment
//!   variable; the value is read from there, so it stays out of `ps` output and
//!   shell history.
//! - **Failures are loud.** [`AppSettings::save_result`] is used rather than
//!   `save()`, because a container with no writable config directory is a
//!   likely outcome, not an exotic one, and "added" without a written file is
//!   the worst possible answer.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};

use octa::cloud::{CloudConnection, CloudKind, CloudSecret};
use octa::db::{DbAuth, DbConnection, DbEngine};
use octa::ui::settings::AppSettings;

/// Parse `key=value,key=value` into a map, lowercasing keys and trimming both
/// sides. Values may not contain a comma; nothing Octa stores does (endpoints,
/// hosts and prefixes are all comma-free), and rejecting is better than a
/// quoting mini-language nobody remembers.
pub fn parse_spec(spec: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| anyhow!("`{part}` is not key=value"))?;
        let k = k.trim().to_ascii_lowercase();
        if k.is_empty() {
            bail!("empty key in `{part}`");
        }
        if out.insert(k.clone(), v.trim().to_string()).is_some() {
            bail!("`{k}` given twice");
        }
    }
    if out.is_empty() {
        bail!("empty spec (expected key=value,key=value)");
    }
    Ok(out)
}

/// Read a `true`/`false`-ish value, defaulting when the key is absent.
fn flag(spec: &BTreeMap<String, String>, key: &str, default: bool) -> Result<bool> {
    match spec.get(key) {
        None => Ok(default),
        Some(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            other => bail!("`{key}={other}` is not a true/false value"),
        },
    }
}

fn opt(spec: &BTreeMap<String, String>, key: &str) -> Option<String> {
    spec.get(key).filter(|v| !v.is_empty()).cloned()
}

fn required(spec: &BTreeMap<String, String>, key: &str) -> Result<String> {
    opt(spec, key).ok_or_else(|| anyhow!("`{key}=` is required"))
}

/// Reject keys this version does not know, so a typo (`buckett=`) fails loudly
/// instead of creating a connection that silently points nowhere.
fn reject_unknown(spec: &BTreeMap<String, String>, known: &[&str]) -> Result<()> {
    let unknown: Vec<&str> = spec
        .keys()
        .map(String::as_str)
        .filter(|k| !known.contains(k))
        .collect();
    if !unknown.is_empty() {
        bail!(
            "unknown key(s): {}. Known keys: {}",
            unknown.join(", "),
            known.join(", ")
        );
    }
    Ok(())
}

/// `kind=` values that mean a cloud connection, and their [`CloudKind`].
fn cloud_kind(name: &str) -> Option<CloudKind> {
    match name.to_ascii_lowercase().as_str() {
        "s3" | "aws" => Some(CloudKind::S3),
        "azure" | "az" | "azureblob" | "abfs" => Some(CloudKind::AzureBlob),
        "gcs" | "gs" | "google" => Some(CloudKind::Gcs),
        _ => None,
    }
}

/// `kind=` values that mean a database connection.
fn db_engine(name: &str) -> Option<DbEngine> {
    match name.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "pg" => Some(DbEngine::Postgres),
        "mysql" | "mariadb" => Some(DbEngine::MySql),
        "mssql" | "sqlserver" => Some(DbEngine::Mssql),
        "oracle" => Some(DbEngine::Oracle),
        "redshift" => Some(DbEngine::Redshift),
        "clickhouse" => Some(DbEngine::ClickHouse),
        "exasol" => Some(DbEngine::Exasol),
        "trino" => Some(DbEngine::Trino),
        "athena" => Some(DbEngine::Athena),
        "snowflake" => Some(DbEngine::Snowflake),
        "databricks" => Some(DbEngine::Databricks),
        "bigquery" | "bq" => Some(DbEngine::BigQuery),
        _ => None,
    }
}

const CLOUD_KEYS: &[&str] = &[
    "kind",
    "name",
    "bucket",
    "region",
    "endpoint",
    "prefix",
    "account",
    "profile",
    "account_level",
    "anonymous",
    "allow_writes",
    "force_path_style",
    "allow_http",
];

const DB_KEYS: &[&str] = &[
    "kind",
    "name",
    "host",
    "port",
    "database",
    "user",
    "allow_writes",
    "query_timeout",
];

/// Turn the value of `--secret-env`'s variable into a [`CloudSecret`].
///
/// Accepts the JSON form Octa itself stores (for anything exotic), else a
/// per-kind shorthand: `ACCESS_KEY:SECRET_KEY[:TOKEN]` for S3, and for Azure
/// either an account key or a SAS token (recognised by its `sig=` parameter).
pub fn parse_cloud_secret(kind: CloudKind, raw: &str) -> Result<CloudSecret> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("the secret environment variable is empty");
    }
    if let Ok(parsed) = serde_json::from_str::<CloudSecret>(raw) {
        return Ok(parsed);
    }
    match kind {
        CloudKind::S3 => {
            let mut parts = raw.splitn(3, ':');
            let access_key_id = parts.next().unwrap_or_default().trim().to_string();
            let secret_access_key = parts
                .next()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow!("S3 secret must be ACCESS_KEY_ID:SECRET_ACCESS_KEY[:TOKEN]")
                })?;
            if access_key_id.is_empty() {
                bail!("S3 secret must be ACCESS_KEY_ID:SECRET_ACCESS_KEY[:TOKEN]");
            }
            Ok(CloudSecret::S3 {
                access_key_id,
                secret_access_key,
                token: parts
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            })
        }
        CloudKind::AzureBlob => {
            if raw.contains("sig=") || raw.starts_with('?') {
                Ok(CloudSecret::AzureSas(
                    raw.trim_start_matches('?').to_string(),
                ))
            } else {
                Ok(CloudSecret::AzureKey(raw.to_string()))
            }
        }
        CloudKind::Gcs => bail!(
            "GCS connections use application-default credentials or browser sign-in, not a \
             stored secret. Run `gcloud auth application-default login`, or add the connection \
             without --secret-env."
        ),
    }
}

/// The secret string named by `--secret-env`, if given.
fn secret_value(secret_env: Option<&str>) -> Result<Option<String>> {
    let Some(var) = secret_env else {
        return Ok(None);
    };
    let value = std::env::var(var).with_context(|| {
        format!("environment variable {var} is not set (named by --secret-env)")
    })?;
    Ok(Some(value))
}

/// `--add-connection SPEC`. Adds or replaces by name and prints what it did.
pub fn add(spec: String, secret_env: Option<String>) -> Result<()> {
    let spec = parse_spec(&spec)?;
    let kind = required(&spec, "kind")?;
    let name = required(&spec, "name")?;
    let secret = secret_value(secret_env.as_deref())?;
    let mut settings = AppSettings::load();

    let target = if let Some(cloud) = cloud_kind(&kind) {
        reject_unknown(&spec, CLOUD_KEYS)?;
        add_cloud(&mut settings, cloud, &name, &spec, secret.as_deref())?
    } else if let Some(engine) = db_engine(&kind) {
        reject_unknown(&spec, DB_KEYS)?;
        add_db(&mut settings, engine, &name, &spec, secret.as_deref())?
    } else {
        bail!(
            "unknown kind `{kind}`. Cloud: s3, azure, gcs. Database: postgres, mysql, mssql, \
             oracle, redshift, clickhouse, exasol, trino, athena, snowflake, \
             databricks, bigquery."
        );
    };

    let path = settings.save_result().map_err(|e| anyhow!(e))?;
    eprintln!("{target}\nsaved to {}", path.display());
    Ok(())
}

fn add_cloud(
    settings: &mut AppSettings,
    kind: CloudKind,
    name: &str,
    spec: &BTreeMap<String, String>,
    secret: Option<&str>,
) -> Result<String> {
    let account_level = flag(spec, "account_level", false)?;
    let bucket = opt(spec, "bucket").unwrap_or_default();
    if bucket.is_empty() && !account_level {
        bail!("`bucket=` is required unless `account_level=true`");
    }
    if kind == CloudKind::AzureBlob && opt(spec, "account").is_none() {
        bail!("Azure needs `account=` (the storage account name)");
    }
    let endpoint = opt(spec, "endpoint");
    // Custom S3-compatible endpoints almost always need path-style addressing;
    // real AWS does not. Same default the Settings form applies.
    let force_path_style = flag(spec, "force_path_style", endpoint.is_some())?;

    // Replacing by name keeps the id (and therefore the keyring entry) stable,
    // so re-running a provisioning script is idempotent rather than additive.
    let existing = settings
        .cloud_connections
        .iter()
        .position(|c| c.name == name);
    // Same id scheme the Settings form uses, so a connection added here and
    // one added in the GUI are indistinguishable afterwards.
    let id = existing
        .map(|i| settings.cloud_connections[i].id.clone())
        .unwrap_or_else(|| format!("cloud-{name}"));

    let conn = CloudConnection {
        id: id.clone(),
        name: name.to_string(),
        kind,
        bucket,
        region: opt(spec, "region"),
        endpoint,
        force_path_style,
        allow_http: flag(spec, "allow_http", false)?,
        secret_ref: None,
        account: opt(spec, "account"),
        profile: opt(spec, "profile"),
        anonymous: flag(spec, "anonymous", false)?,
        project: None,
        oauth_client_id: None,
        oauth_tenant: None,
        prefix: opt(spec, "prefix").map(|p| if p.ends_with('/') { p } else { format!("{p}/") }),
        account_level,
        allow_writes: flag(spec, "allow_writes", false)?,
    };
    let verb = if existing.is_some() {
        "updated"
    } else {
        "added"
    };
    match existing {
        Some(i) => settings.cloud_connections[i] = conn,
        None => settings.cloud_connections.push(conn),
    }

    let mut note = format!("{verb} cloud connection `{name}` (id {id})");
    if let Some(raw) = secret {
        let parsed = parse_cloud_secret(kind, raw)?;
        match octa::ui::settings::cloud_secrets::set_cloud_secret(&id, &parsed, settings) {
            Ok(true) => note.push_str("\nsecret stored in the OS keyring"),
            Ok(false) => note.push_str(
                "\nsecret stored in settings.toml as plain text (no OS keyring available)",
            ),
            Err(e) => bail!("storing the secret: {e}"),
        }
    }
    Ok(note)
}

fn add_db(
    settings: &mut AppSettings,
    engine: DbEngine,
    name: &str,
    spec: &BTreeMap<String, String>,
    secret: Option<&str>,
) -> Result<String> {
    let port = match opt(spec, "port") {
        Some(p) => p
            .parse::<u16>()
            .with_context(|| format!("`port={p}` is not a port number"))?,
        None => engine.default_port(),
    };
    let existing = settings.db_connections.iter().position(|c| c.name == name);
    let id = existing
        .map(|i| settings.db_connections[i].id.clone())
        .unwrap_or_else(DbConnection::fresh_id);

    let conn =
        DbConnection {
            id: id.clone(),
            name: name.to_string(),
            engine,
            host: required(spec, "host")?,
            port,
            database: opt(spec, "database").unwrap_or_default(),
            username: opt(spec, "user").unwrap_or_default(),
            // Password is the only auth a spec can express: every other method
            // (AWS IAM, Azure AD, key-pair JWT, browser sign-in) needs fields and
            // interactive steps that do not belong in a one-line flag.
            auth: DbAuth::Password,
            allow_writes: flag(spec, "allow_writes", false)?,
            oauth_client_id: None,
            oauth_tenant: None,
            // Same reasoning as `auth`: a jump host needs a host, a user, a key
            // path and possibly a passphrase in the keyring. Configure it in
            // Settings -> Databases; the CLI then uses it like any other surface.
            athena_workgroup: None,
            athena_output_location: None,
            query_timeout_secs: match opt(spec, "query_timeout") {
                Some(v) => v.parse::<u32>().ok().filter(|n| *n >= 1).with_context(|| {
                    format!("`query_timeout={v}` is not a whole number of seconds")
                })?,
                None => octa::db::DEFAULT_QUERY_TIMEOUT_SECS,
            },
            ssh: None,
            tunnel_port: None,
        };
    let verb = if existing.is_some() {
        "updated"
    } else {
        "added"
    };
    match existing {
        Some(i) => settings.db_connections[i] = conn,
        None => settings.db_connections.push(conn),
    }

    let mut note = format!("{verb} database connection `{name}` (id {id})");
    if let Some(raw) = secret {
        match octa::ui::settings::db_secrets::set_db_secret(&id, raw, settings) {
            Ok(true) => note.push_str("\nsecret stored in the OS keyring"),
            Ok(false) => note.push_str(
                "\nsecret stored in settings.toml as plain text (no OS keyring available)",
            ),
            Err(e) => bail!("storing the secret: {e}"),
        }
    }
    Ok(note)
}

/// `--remove-connection NAME` (or id). Removes from both lists and drops the
/// stored secret, so nothing is orphaned in the keyring.
pub fn remove(name_or_id: String) -> Result<()> {
    let mut settings = AppSettings::load();
    let mut removed = Vec::new();

    if let Some(i) = settings
        .cloud_connections
        .iter()
        .position(|c| c.name == name_or_id || c.id == name_or_id)
    {
        let c = settings.cloud_connections.remove(i);
        octa::ui::settings::cloud_secrets::delete_cloud_secret(&c.id, &mut settings);
        removed.push(format!("cloud connection `{}` (id {})", c.name, c.id));
    }
    if let Some(i) = settings
        .db_connections
        .iter()
        .position(|c| c.name == name_or_id || c.id == name_or_id)
    {
        let c = settings.db_connections.remove(i);
        octa::ui::settings::db_secrets::delete_db_secret(&c.id, &mut settings);
        removed.push(format!("database connection `{}` (id {})", c.name, c.id));
    }
    if removed.is_empty() {
        bail!("no saved connection called `{name_or_id}` (try --list-connections)");
    }
    let path = settings.save_result().map_err(|e| anyhow!(e))?;
    eprintln!(
        "removed {}\nsaved to {}",
        removed.join(", "),
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_parses_pairs_and_rejects_junk() {
        let s = parse_spec("kind=s3, name=Prod , bucket=my-bucket").unwrap();
        assert_eq!(s["kind"], "s3");
        assert_eq!(s["name"], "Prod");
        assert_eq!(s["bucket"], "my-bucket");
        // Keys are case-insensitive, values are not.
        assert_eq!(parse_spec("KIND=s3,name=X").unwrap()["kind"], "s3");

        assert!(parse_spec("").is_err());
        assert!(parse_spec("just-a-word").is_err());
        assert!(parse_spec("=novalue").is_err());
        assert!(parse_spec("kind=s3,kind=gcs").is_err(), "duplicate key");
    }

    #[test]
    fn unknown_keys_are_reported_not_ignored() {
        // A typo must not produce a connection pointing nowhere.
        let s = parse_spec("kind=s3,name=x,buckett=y").unwrap();
        let err = reject_unknown(&s, CLOUD_KEYS).unwrap_err().to_string();
        assert!(err.contains("buckett"), "{err}");
        assert!(err.contains("bucket"), "the message lists the real keys");
    }

    #[test]
    fn flags_take_the_usual_spellings() {
        let s = parse_spec("a=true,b=no,c=1,d=off").unwrap();
        assert!(flag(&s, "a", false).unwrap());
        assert!(!flag(&s, "b", true).unwrap());
        assert!(flag(&s, "c", false).unwrap());
        assert!(!flag(&s, "d", true).unwrap());
        assert!(flag(&s, "missing", true).unwrap(), "absent = default");
        let bad = parse_spec("a=maybe").unwrap();
        assert!(flag(&bad, "a", false).is_err());
    }

    #[test]
    fn s3_secret_accepts_the_shorthand_and_the_stored_json() {
        let s = parse_cloud_secret(CloudKind::S3, "AKIA123:supersecret").unwrap();
        assert_eq!(
            s,
            CloudSecret::S3 {
                access_key_id: "AKIA123".into(),
                secret_access_key: "supersecret".into(),
                token: None,
            }
        );
        // With a session token.
        let s = parse_cloud_secret(CloudKind::S3, "AKIA123:supersecret:tok").unwrap();
        assert!(matches!(s, CloudSecret::S3 { token: Some(t), .. } if t == "tok"));
        // Missing half is an error, not a silent empty key.
        assert!(parse_cloud_secret(CloudKind::S3, "AKIA123").is_err());
        assert!(parse_cloud_secret(CloudKind::S3, "  ").is_err());

        // The JSON Octa itself stores round-trips, for anything exotic.
        let json = serde_json::to_string(&CloudSecret::AzureSas("sv=x&sig=y".into())).unwrap();
        assert_eq!(
            parse_cloud_secret(CloudKind::AzureBlob, &json).unwrap(),
            CloudSecret::AzureSas("sv=x&sig=y".into())
        );
    }

    #[test]
    fn azure_tells_a_sas_from_an_account_key() {
        assert!(matches!(
            parse_cloud_secret(CloudKind::AzureBlob, "?sv=2021&sig=abc").unwrap(),
            CloudSecret::AzureSas(_)
        ));
        assert!(matches!(
            parse_cloud_secret(CloudKind::AzureBlob, "base64accountkey==").unwrap(),
            CloudSecret::AzureKey(_)
        ));
    }

    #[test]
    fn gcs_says_why_it_takes_no_secret() {
        let err = parse_cloud_secret(CloudKind::Gcs, "anything")
            .unwrap_err()
            .to_string();
        assert!(err.contains("application-default"), "{err}");
    }
}
