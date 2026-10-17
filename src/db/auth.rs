//! Credential resolution for live DB connections. Password auth reads the
//! stored secret (resolved by the caller from the keyring); the token modes
//! shell out to the vendor CLI, mirroring how the cloud connectors sign in
//! (`aws sso login` / `az login` do the browser part, we just mint tokens).
//!
//! Shell-outs block: call this on a worker thread, never the UI thread.

use std::process::Command;

use anyhow::{Context, Result, bail};

use super::{DbAuth, DbConnection, DbEngine};

pub use crate::auth::oauth_browser::{CachedToken, token_still_valid};
use crate::auth::oauth_browser::{
    OAuthBrowserConfig, bind_ephemeral_listener, percent_decode, unix_now,
};

/// The CLI invocation a token-auth mode runs, split out pure for tests.
/// `None` for password auth (no command involved).
pub fn auth_command(conn: &DbConnection) -> Option<(String, Vec<String>)> {
    match &conn.auth {
        DbAuth::Password => None,
        DbAuth::AwsIam { region, .. } => {
            let mut args = vec![
                "rds".to_string(),
                "generate-db-auth-token".to_string(),
                "--hostname".to_string(),
                conn.host.clone(),
                "--port".to_string(),
                conn.port.to_string(),
                "--username".to_string(),
                conn.username.clone(),
            ];
            if let Some(r) = region
                && !r.trim().is_empty()
            {
                args.push("--region".to_string());
                args.push(r.trim().to_string());
            }
            Some(("aws".to_string(), args))
        }
        DbAuth::AzureAd => Some((
            "az".to_string(),
            vec![
                "account".to_string(),
                "get-access-token".to_string(),
                "--resource".to_string(),
                azure_resource_for(conn.engine).to_string(),
                "--query".to_string(),
                "accessToken".to_string(),
                "-o".to_string(),
                "tsv".to_string(),
            ],
        )),
        DbAuth::GcpIam => Some((
            "gcloud".to_string(),
            vec![
                "sql".to_string(),
                "generate-login-token".to_string(),
                "--format=value(token)".to_string(),
            ],
        )),
        // Warehouse auth kinds don't ride the vendor-CLI token flow: JWT is
        // minted locally, OAuth uses a token URL / browser round-trip, GCP
        // uses ADC. Their resolution lands with each connector (provisional).
        DbAuth::Token
        | DbAuth::KeyPairJwt { .. }
        | DbAuth::OAuthClientCredentials { .. }
        | DbAuth::OAuthBrowser
        | DbAuth::GcpAdc
        | DbAuth::GcpServiceAccount { .. } => None,
    }
}

/// The vendor CLI's *interactive login* command: the one that opens a browser
/// and leaves the CLI signed in, so the token command in [`auth_command`]
/// starts working. `None` for auth modes that have no vendor CLI.
///
/// This is what makes "pick browser sign-in and it just opens the browser"
/// possible without anyone typing a client id: the vendor's own CLI is the
/// registered OAuth application, so Octa never needs one of its own. It is the
/// same route DBeaver's "Default credentials" takes.
pub fn cli_login_command(conn: &DbConnection) -> Option<(&'static str, Vec<String>)> {
    match &conn.auth {
        DbAuth::AzureAd => Some(("az", vec!["login".to_string()])),
        DbAuth::GcpIam => Some(("gcloud", vec!["auth".to_string(), "login".to_string()])),
        DbAuth::AwsIam { .. } => Some(("aws", vec!["sso".to_string(), "login".to_string()])),
        _ => None,
    }
}

/// How long a token minted straight after a CLI login is treated as good for.
///
/// Deliberately conservative. Azure and Google both issue roughly one-hour
/// access tokens but neither is promised, and `az account get-access-token
/// --query accessToken` hands back the token without its expiry. Under-guessing
/// costs one extra CLI call; over-guessing would hand a dead token to a
/// connection attempt.
const CLI_TOKEN_VALIDITY_SECS: u64 = 45 * 60;

/// How to install the vendor CLI on *this* machine. Deliberately not
/// translated: these are shell commands and vendor URLs, and a translated
/// package name would not work when pasted.
pub fn cli_install_command(bin: &str) -> &'static str {
    match (bin, std::env::consts::OS) {
        ("az", "windows") => "winget install -e --id Microsoft.AzureCLI",
        ("az", "macos") => "brew install azure-cli",
        ("az", _) => "https://learn.microsoft.com/cli/azure/install-azure-cli-linux",
        ("gcloud", "windows") => "winget install -e --id Google.CloudSDK",
        ("gcloud", "macos") => "brew install --cask google-cloud-sdk",
        ("gcloud", _) => "https://cloud.google.com/sdk/docs/install",
        ("aws", "windows") => "winget install -e --id Amazon.AWSCLI",
        ("aws", "macos") => "brew install awscli",
        ("aws", _) => {
            "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html"
        }
        _ => "",
    }
}

/// The message shown when the vendor CLI is not installed: what is missing,
/// how to install it on this OS, and the way out that needs no CLI at all.
pub fn cli_missing_message(bin: &str) -> String {
    crate::i18n::t("db.cli_missing")
        .replace("{cli}", bin)
        .replace("{install}", cli_install_command(bin))
}

/// Run the vendor CLI's interactive login. The CLI opens the browser itself
/// and this call does not return until the user has finished there, so it must
/// run off the UI thread.
///
/// Output is captured rather than inherited: a GUI launched from a desktop
/// entry has no terminal for the CLI to print to, so capturing it is the only
/// way its message can reach the user - and it does, through the error text,
/// which the settings dialog renders selectable.
pub fn cli_login(conn: &DbConnection) -> Result<()> {
    let Some((bin, args)) = cli_login_command(conn) else {
        bail!(
            "connection '{}' has no vendor CLI to sign in with",
            conn.name
        )
    };
    match Command::new(bin).args(&args).output() {
        // The CLI is not installed. This is the one failure with a concrete
        // remedy, so it gets the OS-specific instructions rather than a raw
        // "No such file or directory".
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!("{}", cli_missing_message(bin)),
        Err(e) => Err(e).with_context(|| format!("running `{bin} {}`", args.join(" "))),
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            bail!("`{bin} {}` did not finish: {detail}", args.join(" "))
        }
        Ok(_) => Ok(()),
    }
}

/// Sign in for this connection through the vendor CLI, then mint and cache a
/// token so the rest of Octa sees the connection as signed in.
///
/// Blocks on the browser round-trip; call from a worker thread.
pub fn cli_browser_signin(conn: &DbConnection) -> Result<()> {
    cli_login(conn)?;
    let token = resolve_password(conn, None)
        .context("signing in worked, but minting a database token afterwards did not")?;
    cache_browser_token(
        &conn.id,
        CachedToken {
            access_token: token,
            expires_at_unix: unix_now().saturating_add(CLI_TOKEN_VALIDITY_SECS),
        },
    );
    Ok(())
}

/// Sign `conn` in through the browser and cache the token, so every connect
/// path uses it. Blocks until the browser round trip ends: call it on a
/// worker. Shared by Settings' **Sign in with browser** and the sidebar.
/// `client_secret` is the Google client secret, when the connection has one.
pub fn browser_signin(
    conn: &DbConnection,
    client_secret: Option<&str>,
) -> std::result::Result<(), String> {
    let open = crate::auth::oauth_browser::open_url_in_browser;
    let signed_in = if conn.auth == DbAuth::OAuthBrowser && conn.engine == DbEngine::Snowflake {
        // Snowflake's own EXTERNALBROWSER flow, not a generic OAuth client.
        snowflake_sso_token(crate::db::snowflake::account(conn), &conn.username, open)
            .map(|token| cache_browser_token(&conn.id, token))
    } else if aws_sso_configured(conn) {
        // AWS Identity Center uses the device-authorization flow, not the
        // generic PKCE loopback config.
        aws_sso_signin(conn, open).map(|token| cache_browser_token(&conn.id, token))
    } else {
        match browser_oauth_config(conn, client_secret) {
            // A client id is configured: use our own browser flow, which
            // needs no command-line tool at all. Someone who went to the
            // trouble of registering an app gets to use it.
            Some(cfg) => crate::auth::oauth_browser::acquire_token(&cfg, open)
                .map(|token| cache_browser_token(&conn.id, token)),
            // No client id, which is the normal case: hand the browser
            // round-trip to the vendor's own CLI. It is a registered OAuth
            // application already, so nobody has to supply an id.
            None => cli_browser_signin(conn),
        }
    };
    signed_in.map_err(|e| format!("{e:#}"))
}

/// Forget the cached browser token for a connection (the Sign out button),
/// here and in the keyring.
pub fn forget_browser_token(conn_id: &str) {
    crate::ui::settings::db_secrets::delete_oauth_cache(conn_id);
    if let Ok(mut c) = browser_token_cache().lock() {
        c.remove(conn_id);
    }
}

/// Whole minutes a cached browser token has left, or `None` when there is no
/// usable token. Drives the "expires in n min" line beside the Sign in button.
pub fn browser_token_minutes_left(conn_id: &str) -> Option<u64> {
    let t = cached_browser_token(conn_id)?;
    Some(t.expires_at_unix.saturating_sub(unix_now()) / 60)
}

/// The Microsoft Entra (Azure AD) token audience for an engine's `az account
/// get-access-token --resource`. SQL Server and Azure Databricks each have
/// their own; everything else defaults to the shared "OSS RDBMS" resource of
/// Azure Database for PostgreSQL / MySQL flexible servers.
fn azure_resource_for(engine: super::DbEngine) -> &'static str {
    match engine {
        super::DbEngine::Mssql => "https://database.windows.net/",
        // Azure Databricks programmatic-access application id.
        super::DbEngine::Databricks => "2ff814a6-3304-4ab8-85cb-cd0e6f879c1d",
        _ => "https://ossrdbms-aad.database.windows.net",
    }
}

/// The Azure OAuth scope a browser access token needs for this engine (the
/// `.default` form of the engine's token resource).
fn azure_browser_scope(engine: DbEngine) -> &'static str {
    match engine {
        DbEngine::Mssql => "https://database.windows.net/.default",
        // Postgres / MySQL (Azure Database) and anything else map to the
        // OSS-RDBMS resource.
        _ => "https://ossrdbms-aad.database.windows.net/.default",
    }
}

/// Build the browser sign-in config for a connection, or None when no client id
/// is configured (browser sign-in not set up) or the auth kind is not a
/// browser-capable AAD/GCP mode. `client_secret` is the stored Google client
/// secret (Azure ignores it: public clients use PKCE alone).
pub fn browser_oauth_config(
    conn: &DbConnection,
    client_secret: Option<&str>,
) -> Option<OAuthBrowserConfig> {
    // Databricks user-to-machine OAuth uses the workspace's own OIDC endpoints
    // and a public client; the built-in `databricks-cli` client id needs no
    // registration, so it is the default when the user sets none.
    if matches!(conn.auth, DbAuth::OAuthBrowser) && conn.engine == DbEngine::Databricks {
        let host = conn.host.trim().trim_end_matches('/');
        if host.is_empty() {
            return None;
        }
        let client_id = conn
            .oauth_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("databricks-cli")
            .to_string();
        return Some(OAuthBrowserConfig {
            authorize_url: format!("https://{host}/oidc/v1/authorize"),
            token_url: format!("https://{host}/oidc/v1/token"),
            client_id,
            client_secret: None, // public client + PKCE
            scope: "all-apis offline_access".to_string(),
            extra_auth_params: vec![],
        });
    }
    let client_id = conn
        .oauth_client_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())?;
    match conn.auth {
        DbAuth::AzureAd => {
            let tenant = conn
                .oauth_tenant
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("organizations");
            let base = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0");
            Some(OAuthBrowserConfig {
                authorize_url: format!("{base}/authorize"),
                token_url: format!("{base}/token"),
                client_id: client_id.to_string(),
                client_secret: None, // public client + PKCE
                scope: azure_browser_scope(conn.engine).to_string(),
                extra_auth_params: vec![],
            })
        }
        // GCP IAM (Cloud SQL) and BigQuery both authenticate with a Google
        // access token; cloud-platform covers both.
        DbAuth::GcpIam => Some(OAuthBrowserConfig {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            client_id: client_id.to_string(),
            client_secret: client_secret.map(str::to_string),
            scope: "https://www.googleapis.com/auth/cloud-platform".into(),
            extra_auth_params: vec![],
        }),
        _ => None,
    }
}

/// Mint a Google Cloud access token from Application Default Credentials via
/// `gcloud auth application-default print-access-token`. The browser part is
/// the user having run `gcloud auth application-default login`.
///
/// Blocks (shells out); call off the UI thread.
pub fn gcp_adc_token() -> Result<String> {
    let output = Command::new("gcloud")
        .args(["auth", "application-default", "print-access-token"])
        .output()
        .context("running `gcloud` (is the CLI installed and on PATH?)")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`gcloud` failed to print an ADC token: {}; run \
             `gcloud auth application-default login` and retry",
            stderr.trim()
        );
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        bail!("`gcloud` returned an empty ADC token");
    }
    Ok(token)
}

/// Extract the `token=` value from a raw HTTP request (Snowflake's IdP redirect
/// carries it either in the request-line query or the form body).
fn token_from_request(raw: &str) -> Option<String> {
    raw.split(['?', '&', '\n', '\r', ' '])
        .find_map(|part| part.trim().strip_prefix("token="))
        .map(percent_decode)
}

/// Complete the Snowflake external-browser (SSO) login: post an authenticator
/// request to learn the IdP URL + proof key, open the browser, catch the
/// redirect on a loopback port, then exchange the returned token for a session
/// token. Returns the session token and its validity as a [`CachedToken`].
///
/// The browser round-trip cannot be unit-tested (only [`bind_ephemeral_listener`]
/// is); this follows Snowflake's documented EXTERNALBROWSER protocol and is
/// exercised manually / by the live test.
///
/// Blocks on the network and on the user completing the browser sign-in; call
/// off the UI thread.
pub fn snowflake_sso_token(
    account: &str,
    user: &str,
    open_browser: impl Fn(&str),
) -> Result<CachedToken> {
    use std::io::{Read, Write};

    let (listener, port) = bind_ephemeral_listener()?;
    let base = format!("https://{account}.snowflakecomputing.com");
    let account_upper = account.to_uppercase();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let post = |url: &str, body: &serde_json::Value| -> Result<serde_json::Value> {
        let mut resp = agent
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(body)
            .with_context(|| format!("posting to {url}"))?;
        let status = resp.status();
        let text = resp
            .body_mut()
            .read_to_string()
            .context("reading response")?;
        if !status.is_success() {
            bail!("Snowflake HTTP {}: {}", status.as_u16(), text.trim());
        }
        serde_json::from_str(&text).context("parsing Snowflake response")
    };

    // 1. Authenticator request -> SSO URL + proof key.
    let auth_req = serde_json::json!({
        "data": {
            "ACCOUNT_NAME": account_upper,
            "LOGIN_NAME": user,
            "AUTHENTICATOR": "EXTERNALBROWSER",
            "BROWSER_MODE_REDIRECT_PORT": port.to_string(),
        }
    });
    let auth_resp = post(&format!("{base}/session/authenticator-request"), &auth_req)?;
    let sso_url = auth_resp["data"]["ssoUrl"]
        .as_str()
        .context("Snowflake authenticator response had no ssoUrl")?;
    let proof_key = auth_resp["data"]["proofKey"].as_str().unwrap_or_default();

    // 2. Hand the URL to the browser and wait for the IdP to redirect back.
    open_browser(sso_url);
    let (mut stream, _) = listener
        .accept()
        .context("waiting for the browser redirect")?;
    let mut buf = [0u8; 8192];
    let n = stream
        .read(&mut buf)
        .context("reading the browser redirect")?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let token = token_from_request(&raw)
        .context("the browser redirect carried no token (SSO was cancelled?)")?;
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
          <html><body>Sign-in complete. You may close this tab.</body></html>",
    );

    // 3. Exchange the SSO token for a Snowflake session token.
    let login_req = serde_json::json!({
        "data": {
            "ACCOUNT_NAME": account_upper,
            "LOGIN_NAME": user,
            "AUTHENTICATOR": "EXTERNALBROWSER",
            "TOKEN": token,
            "PROOF_KEY": proof_key,
        }
    });
    let login_resp = post(&format!("{base}/session/v1/login-request"), &login_req)?;
    let session = login_resp["data"]["token"]
        .as_str()
        .context("Snowflake login response had no session token")?
        .to_string();
    let validity = login_resp["data"]["validityInSeconds"]
        .as_u64()
        .unwrap_or(3600);
    Ok(CachedToken {
        access_token: session,
        expires_at_unix: unix_now().saturating_add(validity),
    })
}

/// Resolve the connection's effective password/token. `stored` is the
/// keyring/fallback secret (used by `Password` auth only); the token modes
/// mint a fresh token per call via the vendor CLI.
pub fn resolve_password(conn: &DbConnection, stored: Option<&str>) -> Result<String> {
    match &conn.auth {
        DbAuth::Password => stored
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .with_context(|| {
                format!(
                    "no stored password for connection '{}'; set one in Settings -> Databases",
                    conn.name
                )
            }),
        DbAuth::AwsIam { .. } => {
            // Identity Center connections mint the RDS token from role
            // credentials fetched with the browser-cached SSO token; plain
            // AwsIam falls back to ambient credentials via the CLI.
            if aws_sso_config(conn).is_some() {
                match cached_browser_token(&conn.id) {
                    Some(t) => {
                        let creds = aws_role_credentials(conn, &t.access_token)?;
                        aws_rds_token_with_creds(conn, &creds)
                    }
                    None => bail!(
                        "sign in to AWS IAM Identity Center in Settings -> Databases \
                         for '{}', then retry",
                        conn.name
                    ),
                }
            } else {
                run_token_command(conn, "run `aws sso login` (or `aws configure`) and retry")
            }
        }
        // A browser token cached for this connection (from the GUI sign-in
        // flow) is used before shelling out to the vendor CLI.
        DbAuth::AzureAd => match cached_browser_token(&conn.id) {
            Some(t) => Ok(t.access_token),
            None => run_token_command(
                conn,
                "run `az login`, or sign in with your browser in Settings, then retry",
            ),
        },
        DbAuth::GcpIam => match cached_browser_token(&conn.id) {
            Some(t) => Ok(t.access_token),
            None => run_token_command(
                conn,
                "run `gcloud auth login`, or sign in with your browser in Settings, then retry",
            ),
        },
        DbAuth::Token
        | DbAuth::KeyPairJwt { .. }
        | DbAuth::OAuthClientCredentials { .. }
        | DbAuth::OAuthBrowser
        | DbAuth::GcpAdc
        | DbAuth::GcpServiceAccount { .. } => bail!(
            "{} auth is not yet implemented",
            conn.auth.kind().i18n_key()
        ),
    }
}

fn run_token_command(conn: &DbConnection, login_hint: &str) -> Result<String> {
    let (bin, args) = auth_command(conn).expect("token auth modes have a command");
    let output = match Command::new(&bin).args(&args).output() {
        // "No such file or directory" tells the user nothing they can act on.
        // A missing CLI is the one failure here with a concrete remedy, so it
        // gets the install command for this OS and the no-CLI alternative.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("{}", cli_missing_message(&bin))
        }
        Err(e) => return Err(e).with_context(|| format!("running `{bin}`")),
        Ok(out) => out,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`{bin}` failed for connection '{}': {}; {login_hint}",
            conn.name,
            stderr.trim()
        );
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        bail!("`{bin}` returned an empty token; {login_hint}");
    }
    Ok(token)
}

/// Process-global cache of browser access tokens, keyed by connection id.
/// ponytail: one global lock is fine for a handful of connections; shard by
/// id if it ever matters.
fn browser_token_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, CachedToken>>
{
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, CachedToken>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Store a browser access token for a connection (used by the GUI sign-in
/// flow). It goes into the keyring as well, so a restart, the CLI and the
/// MCP server reuse the sign-in until the token expires.
pub fn cache_browser_token(conn_id: &str, token: CachedToken) {
    if let Ok(json) = serde_json::to_string(&token) {
        crate::ui::settings::db_secrets::set_oauth_cache(conn_id, &json);
    }
    if let Ok(mut c) = browser_token_cache().lock() {
        c.insert(conn_id.to_string(), token);
    }
}

/// A still-valid cached browser token for the connection, if any: from this
/// process, else from the keyring (a sign-in from an earlier run or from the
/// GUI while this is the CLI).
pub fn cached_browser_token(conn_id: &str) -> Option<CachedToken> {
    let now = unix_now();
    if let Some(t) = browser_token_cache()
        .lock()
        .ok()?
        .get(conn_id)
        .filter(|t| token_still_valid(t, now))
    {
        return Some(t.clone());
    }
    // The keyring is asked once per connection per run: Settings asks every
    // frame, and a keyring lookup can take a D-Bus round trip.
    static LOOKED_UP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let first_look = LOOKED_UP
        .get_or_init(Default::default)
        .lock()
        .ok()?
        .insert(conn_id.to_string());
    if !first_look {
        return None;
    }
    let stored = crate::ui::settings::db_secrets::get_oauth_cache(conn_id)?;
    let token: CachedToken = serde_json::from_str(&stored).ok()?;
    if !token_still_valid(&token, now) {
        return None;
    }
    if let Ok(mut c) = browser_token_cache().lock() {
        c.insert(conn_id.to_string(), token.clone());
    }
    Some(token)
}

/// Whether a valid browser token is cached (drives the "signed in via browser"
/// note in the UI).
pub fn has_browser_token(conn_id: &str) -> bool {
    cached_browser_token(conn_id).is_some()
}

/// Outcome of resolving a connection's credential when a browser fallback is
/// possible: either a usable secret/token, or a signal that the GUI should
/// prompt the user to sign in via the browser.
#[derive(Debug)]
pub enum PasswordResolution {
    Ready(String),
    NeedsBrowserSignin,
}

/// Whether this connection has a browser sign-in configured (a non-empty
/// client id).
fn has_browser_client(conn: &DbConnection) -> bool {
    conn.oauth_client_id
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
}

/// Decide what to do once the CLI credential path has failed or is missing:
/// prompt for the browser (GUI + client id configured) or error with a hint.
fn after_cli_failure(conn: &DbConnection, has_gui_fallback: bool) -> Result<PasswordResolution> {
    // Two ways to reach a browser: a client id of our own (the native flow), or
    // the vendor CLI's login, which is the registered OAuth app itself. Having
    // either is enough to offer the user a Sign in button; whether the CLI is
    // actually installed is answered by that button, which reports the install
    // command for this OS rather than guessing here.
    let login = cli_login_command(conn);
    if has_gui_fallback && (has_browser_client(conn) || login.is_some()) {
        return Ok(PasswordResolution::NeedsBrowserSignin);
    }
    // Headless (CLI / MCP): no browser to open, so name the exact command to
    // run in a terminal instead.
    match login {
        Some((bin, args)) => bail!(
            "could not obtain a token for '{}': run `{bin} {}` and retry",
            conn.name,
            args.join(" ")
        ),
        None => bail!(
            "could not obtain a token for '{}': set an OAuth client id in this \
             connection's settings",
            conn.name
        ),
    }
}

/// Whether a browser sign-in can give `conn` its credential: Azure AD and
/// GCP IAM (through the vendor CLI's login or an OAuth client), AWS IAM
/// Identity Center, and Databricks' and Snowflake's browser OAuth. The same
/// set Settings offers **Sign in with browser** for.
pub fn browser_signin_offered(conn: &DbConnection) -> bool {
    matches!(conn.auth, DbAuth::AzureAd | DbAuth::GcpIam)
        || aws_sso_configured(conn)
        || browser_only(conn)
}

/// Databricks or Snowflake with browser OAuth: no CLI path at all, and the
/// browser token is the bearer itself.
fn browser_only(conn: &DbConnection) -> bool {
    conn.auth == DbAuth::OAuthBrowser
        && matches!(
            conn.engine,
            crate::db::DbEngine::Databricks | crate::db::DbEngine::Snowflake
        )
}

/// Token-cache-aware credential resolution with a browser fallback. Order:
/// valid cached browser token -> vendor CLI -> browser prompt (GUI) or error
/// (headless). `has_gui_fallback` is false for MCP/CLI so a browser is never
/// requested there (those paths keep today's CLI-only behaviour). AWS IAM
/// Identity Center and Databricks browser OAuth have no CLI step: without a
/// valid cached token they go straight to the prompt.
pub fn resolve_password_with_cache(
    conn: &DbConnection,
    stored: Option<&str>,
    cached: Option<&CachedToken>,
    has_gui_fallback: bool,
) -> Result<PasswordResolution> {
    let valid = cached.filter(|t| token_still_valid(t, unix_now()));
    if aws_sso_configured(conn) || browser_only(conn) {
        let Some(t) = valid else {
            if has_gui_fallback {
                return Ok(PasswordResolution::NeedsBrowserSignin);
            }
            bail!(
                "sign in with your browser in Settings -> Databases for '{}', then retry",
                conn.name
            );
        };
        // The Identity Center token buys role credentials, which mint the
        // RDS token; Databricks and Snowflake take the browser token as their
        // bearer.
        return if browser_only(conn) {
            Ok(PasswordResolution::Ready(t.access_token.clone()))
        } else {
            resolve_password(conn, stored).map(PasswordResolution::Ready)
        };
    }
    // Password auth and non-AAD/GCP token modes are unchanged: no browser path.
    if !matches!(conn.auth, DbAuth::AzureAd | DbAuth::GcpIam) {
        return resolve_password(conn, stored).map(PasswordResolution::Ready);
    }
    // 1. A valid cached browser token wins over any CLI shell-out.
    if let Some(t) = valid {
        return Ok(PasswordResolution::Ready(t.access_token.clone()));
    }
    // 2. Try the CLI (existing path). 3. On failure, prompt or error.
    match resolve_password(conn, stored) {
        Ok(tok) => Ok(PasswordResolution::Ready(tok)),
        Err(_) => after_cli_failure(conn, has_gui_fallback),
    }
}

/// Parse an unencrypted RSA private key from a PKCS#8 (`BEGIN PRIVATE KEY`) or
/// PKCS#1 (`BEGIN RSA PRIVATE KEY`) PEM.
fn parse_rsa_pem(pem: &str) -> Result<rsa::RsaPrivateKey> {
    use rsa::RsaPrivateKey;
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .context("parsing the RSA private key (expected unencrypted PKCS#8/PKCS#1 PEM)")
}

/// Encode + RS256-sign a JWT (`header.claims.signature`, all base64url) with
/// `key`.
fn rs256_jwt(
    key: &rsa::RsaPrivateKey,
    header: &serde_json::Value,
    claims: &serde_json::Value,
) -> Result<String> {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use rsa::pkcs1v15::SigningKey;
    use rsa::sha2::Sha256;
    use rsa::signature::{SignatureEncoding, Signer};

    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(header)?),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?),
    );
    let signing_key = SigningKey::<Sha256>::new(key.clone());
    let signature = signing_key.sign(signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ))
}

/// Build a Snowflake key-pair-auth login JWT (RS256), signed with the RSA
/// private key in `private_key_pem`. `account` and `user` are the Snowflake
/// account identifier and username; both are uppercased into the claims.
///
/// Claims follow Snowflake's spec: `iss = "{ACCOUNT}.{USER}.SHA256:{fp}"`,
/// `sub = "{ACCOUNT}.{USER}"`, where `fp` is the base64 SHA-256 of the public
/// key's DER SubjectPublicKeyInfo. The token is valid for 59 minutes.
///
/// ponytail: encrypted PKCS#8 keys (a `passphrase`) are not yet supported -
/// enable rsa's `pkcs5` feature and use `from_pkcs8_encrypted_pem` to add it.
pub fn snowflake_jwt(
    account: &str,
    user: &str,
    private_key_pem: &[u8],
    passphrase: Option<&str>,
) -> Result<String> {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use rsa::pkcs8::EncodePublicKey;
    use rsa::sha2::{Digest, Sha256};

    if passphrase.is_some_and(|p| !p.is_empty()) {
        bail!(
            "encrypted Snowflake private keys are not yet supported; use an \
             unencrypted PKCS#8 key"
        );
    }
    let pem = std::str::from_utf8(private_key_pem)
        .context("Snowflake private key is not valid UTF-8 PEM")?;
    let key = parse_rsa_pem(pem)?;

    let spki_der = key
        .to_public_key()
        .to_public_key_der()
        .context("encoding the Snowflake public key")?;
    let fingerprint = STANDARD.encode(Sha256::digest(spki_der.as_bytes()));

    let account = account.to_uppercase();
    let user = user.to_uppercase();
    let now = unix_now();
    let header = serde_json::json!({ "alg": "RS256", "typ": "JWT" });
    let claims = serde_json::json!({
        "iss": format!("{account}.{user}.SHA256:{fingerprint}"),
        "sub": format!("{account}.{user}"),
        "iat": now,
        "exp": now + 59 * 60,
    });
    rs256_jwt(&key, &header, &claims)
}

/// Build a Google service-account OAuth assertion (RS256 JWT) for the
/// jwt-bearer grant. `key_json` is a downloaded service-account key file
/// (`client_email`, `token_uri`, `private_key`, optional `private_key_id`).
pub fn gcp_sa_assertion(key_json: &serde_json::Value, scope: &str) -> Result<String> {
    let field = |k: &str| -> Result<&str> {
        key_json
            .get(k)
            .and_then(|v| v.as_str())
            .with_context(|| format!("service-account key missing `{k}`"))
    };
    let client_email = field("client_email")?;
    let token_uri = field("token_uri")?;
    let key = parse_rsa_pem(field("private_key")?)?;

    let now = unix_now();
    // The key id becomes the JWT `kid` header when present.
    let header = match key_json.get("private_key_id").and_then(|v| v.as_str()) {
        Some(kid) => serde_json::json!({ "alg": "RS256", "typ": "JWT", "kid": kid }),
        None => serde_json::json!({ "alg": "RS256", "typ": "JWT" }),
    };
    let claims = serde_json::json!({
        "iss": client_email,
        "scope": scope,
        "aud": token_uri,
        "iat": now,
        "exp": now + 60 * 60,
    });
    rs256_jwt(&key, &header, &claims)
}

/// Exchange a service-account assertion for an OAuth access token: POST the
/// jwt-bearer grant to the key's `token_uri` and return `access_token`.
///
/// Blocks on the network; call off the UI thread.
pub fn gcp_sa_token(key_json: &serde_json::Value, scope: &str) -> Result<String> {
    let assertion = gcp_sa_assertion(key_json, scope)?;
    let token_uri = key_json
        .get("token_uri")
        .and_then(|v| v.as_str())
        .context("service-account key missing `token_uri`")?;
    // grant_type is a fixed URN (colons percent-encoded); the assertion is
    // base64url so it needs no encoding.
    let body = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .post(token_uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(&body)
        .context("posting the GCP token request")?;
    let status = resp.status();
    let text = resp
        .body_mut()
        .read_to_string()
        .context("reading the GCP token response")?;
    if !status.is_success() {
        bail!(
            "GCP token endpoint HTTP {}: {}",
            status.as_u16(),
            text.trim()
        );
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).context("parsing the GCP token response")?;
    v.get("access_token")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .context("GCP token response had no access_token")
}

/// Mint an access token via the OAuth 2.0 client-credentials grant: POST the
/// form to `token_url` and read `access_token` + `expires_in`.
///
/// Blocks on the network; call off the UI thread.
pub fn oauth_client_credentials_token(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scope: Option<&str>,
) -> Result<CachedToken> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];
    if let Some(s) = scope.filter(|s| !s.is_empty()) {
        form.push(("scope", s));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .post(token_url)
        .send_form(form)
        .context("posting the OAuth client-credentials request")?;
    let status = resp.status();
    let text = resp
        .body_mut()
        .read_to_string()
        .context("reading the OAuth token response")?;
    if !status.is_success() {
        bail!(
            "OAuth token endpoint HTTP {}: {}",
            status.as_u16(),
            text.trim()
        );
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).context("parsing the OAuth token response")?;
    let access_token = v
        .get("access_token")
        .and_then(|t| t.as_str())
        .context("OAuth token response had no access_token")?
        .to_string();
    // `expires_in` is seconds-from-now; default to one hour when omitted.
    let expires_in = v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(3600);
    Ok(CachedToken {
        access_token,
        expires_at_unix: unix_now().saturating_add(expires_in),
    })
}

// ---------------------------------------------------------------------------
// AWS IAM Identity Center (SSO) in-app browser sign-in.
//
// Flow: RegisterClient -> StartDeviceAuthorization -> browser -> poll
// CreateToken (device-code grant) -> GetRoleCredentials -> mint the RDS auth
// token from the returned role credentials. The SSO access token is cached per
// connection (like Azure/GCP) so the browser only opens when it expires.
// ---------------------------------------------------------------------------

/// IAM Identity Center settings resolved from an `AwsIam` connection. `None`
/// unless a start URL, account id and role are all present (region falls back
/// to the connection's DB region).
pub(crate) struct AwsSsoConfig<'a> {
    start_url: &'a str,
    region: &'a str,
    account_id: &'a str,
    role: &'a str,
}

pub(crate) fn aws_sso_config(conn: &DbConnection) -> Option<AwsSsoConfig<'_>> {
    let DbAuth::AwsIam {
        region,
        sso_start_url,
        sso_region,
        sso_account_id,
        sso_role,
    } = &conn.auth
    else {
        return None;
    };
    let start_url = sso_start_url.as_deref()?.trim();
    let account_id = sso_account_id.as_deref()?.trim();
    let role = sso_role.as_deref()?.trim();
    if start_url.is_empty() || account_id.is_empty() || role.is_empty() {
        return None;
    }
    let region = sso_region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| region.as_deref().map(str::trim).filter(|s| !s.is_empty()))?;
    Some(AwsSsoConfig {
        start_url,
        region,
        account_id,
        role,
    })
}

/// Whether this connection uses IAM Identity Center (SSO), i.e. it drives the
/// in-app browser device flow rather than ambient AWS credentials.
pub fn aws_sso_configured(conn: &DbConnection) -> bool {
    aws_sso_config(conn).is_some()
}

fn sso_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into()
}

/// Run the IAM Identity Center device-authorization sign-in and return the SSO
/// access token. Blocks on the browser round-trip + polling; call off the UI
/// thread.
pub fn aws_sso_signin(conn: &DbConnection, open_browser: impl Fn(&str)) -> Result<CachedToken> {
    let sso = aws_sso_config(conn)
        .context("AWS IAM Identity Center is not configured for this connection")?;
    let agent = sso_agent();
    let oidc = format!("https://oidc.{}.amazonaws.com", sso.region);
    let post = |url: &str, body: &serde_json::Value| -> Result<(u16, serde_json::Value)> {
        let mut resp = agent
            .post(url)
            .header("Content-Type", "application/json")
            .send_json(body)
            .with_context(|| format!("posting to {url}"))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .context("reading the AWS SSO response")?;
        let v = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok((status, v))
    };

    // 1. Register a public client.
    let (st, reg) = post(
        &format!("{oidc}/client/register"),
        &serde_json::json!({"clientName": "octa", "clientType": "public"}),
    )?;
    if !(200..300).contains(&st) {
        bail!("AWS SSO RegisterClient failed (HTTP {st}): {reg}");
    }
    let client_id = reg["clientId"]
        .as_str()
        .context("RegisterClient response had no clientId")?
        .to_string();
    let client_secret = reg["clientSecret"]
        .as_str()
        .context("RegisterClient response had no clientSecret")?
        .to_string();

    // 2. Start device authorization for the portal start URL.
    let (st, dev) = post(
        &format!("{oidc}/device_authorization"),
        &serde_json::json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "startUrl": sso.start_url,
        }),
    )?;
    if !(200..300).contains(&st) {
        bail!("AWS SSO StartDeviceAuthorization failed (HTTP {st}): {dev}");
    }
    let device_code = dev["deviceCode"]
        .as_str()
        .context("device authorization had no deviceCode")?
        .to_string();
    let verify = dev["verificationUriComplete"]
        .as_str()
        .context("device authorization had no verificationUriComplete")?;
    let mut interval = dev["interval"].as_u64().unwrap_or(5).max(1);
    let expires_in = dev["expiresIn"].as_u64().unwrap_or(600);

    // 3. Open the browser and poll CreateToken until the user approves.
    open_browser(verify);
    let deadline = unix_now().saturating_add(expires_in);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(interval));
        let (st, tok) = post(
            &format!("{oidc}/token"),
            &serde_json::json!({
                "clientId": client_id,
                "clientSecret": client_secret,
                "grantType": "urn:ietf:params:oauth:grant-type:device_code",
                "deviceCode": device_code,
            }),
        )?;
        if (200..300).contains(&st) {
            let access_token = tok["accessToken"]
                .as_str()
                .context("CreateToken response had no accessToken")?
                .to_string();
            let ttl = tok["expiresIn"].as_u64().unwrap_or(3600);
            return Ok(CachedToken {
                access_token,
                expires_at_unix: unix_now().saturating_add(ttl),
            });
        }
        match tok["error"].as_str().unwrap_or("") {
            "authorization_pending" => {}
            "slow_down" => interval = interval.saturating_add(5),
            "" => bail!("AWS SSO sign-in failed: {tok}"),
            other => bail!("AWS SSO sign-in failed: {other}"),
        }
        if unix_now() >= deadline {
            bail!("AWS SSO sign-in timed out; retry");
        }
    }
}

/// Temporary AWS credentials for an assumed role.
pub(crate) struct AwsCreds {
    pub(crate) access_key_id: String,
    pub(crate) secret_access_key: String,
    pub(crate) session_token: String,
}

/// Exchange the SSO access token for role credentials via the Identity Center
/// portal. Blocks on the network; call off the UI thread.
pub(crate) fn aws_role_credentials(
    conn: &DbConnection,
    sso_access_token: &str,
) -> Result<AwsCreds> {
    let sso = aws_sso_config(conn)
        .context("AWS IAM Identity Center is not configured for this connection")?;
    let agent = sso_agent();
    let url = format!(
        "https://portal.sso.{}.amazonaws.com/federation/credentials",
        sso.region
    );
    let mut resp = agent
        .get(&url)
        .query("account_id", sso.account_id)
        .query("role_name", sso.role)
        .header("x-amz-sso_bearer_token", sso_access_token)
        .call()
        .context("fetching AWS role credentials")?;
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .read_to_string()
        .context("reading the AWS role-credentials response")?;
    if !(200..300).contains(&status) {
        bail!(
            "AWS GetRoleCredentials failed (HTTP {status}): {}",
            text.trim()
        );
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).context("parsing the AWS role-credentials response")?;
    let c = &v["roleCredentials"];
    let field = |k: &str| -> Result<String> {
        c[k].as_str()
            .map(str::to_string)
            .with_context(|| format!("role credentials had no {k}"))
    };
    Ok(AwsCreds {
        access_key_id: field("accessKeyId")?,
        secret_access_key: field("secretAccessKey")?,
        session_token: field("sessionToken")?,
    })
}

/// Mint the RDS IAM auth token from the role credentials.
///
/// ponytail: reuses `aws rds generate-db-auth-token` for the SigV4 presign
/// (needs the AWS CLI on PATH, as the plain AwsIam path already does); replace
/// with a native SigV4 presign if dropping the CLI dependency ever matters.
fn aws_rds_token_with_creds(conn: &DbConnection, creds: &AwsCreds) -> Result<String> {
    let (bin, args) = auth_command(conn).expect("AwsIam auth has a token command");
    let output = Command::new(&bin)
        .args(&args)
        .env("AWS_ACCESS_KEY_ID", &creds.access_key_id)
        .env("AWS_SECRET_ACCESS_KEY", &creds.secret_access_key)
        .env("AWS_SESSION_TOKEN", &creds.session_token)
        .output()
        .with_context(|| format!("running `{bin}` (is the AWS CLI installed and on PATH?)"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`{bin}` failed to mint an RDS auth token for '{}': {}",
            conn.name,
            stderr.trim()
        );
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        bail!("`{bin}` returned an empty RDS auth token");
    }
    Ok(token)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
