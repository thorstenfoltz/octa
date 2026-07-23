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
    let output = Command::new(&bin)
        .args(&args)
        .output()
        .with_context(|| format!("running `{bin}` (is the CLI installed and on PATH?)"))?;
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
/// Session-only (MVP: not persisted). ponytail: one global lock is fine for a
/// handful of connections; shard by id if it ever matters.
fn browser_token_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, CachedToken>>
{
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, CachedToken>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Store a browser access token for a connection (used by the GUI sign-in flow).
pub fn cache_browser_token(conn_id: &str, token: CachedToken) {
    if let Ok(mut c) = browser_token_cache().lock() {
        c.insert(conn_id.to_string(), token);
    }
}

/// A still-valid cached browser token for the connection, if any.
pub fn cached_browser_token(conn_id: &str) -> Option<CachedToken> {
    let c = browser_token_cache().lock().ok()?;
    c.get(conn_id)
        .filter(|t| token_still_valid(t, unix_now()))
        .cloned()
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
    if has_browser_client(conn) {
        if has_gui_fallback {
            Ok(PasswordResolution::NeedsBrowserSignin)
        } else {
            bail!(
                "could not obtain a token for '{}': no browser available (headless); \
                 run the vendor CLI login and retry",
                conn.name
            )
        }
    } else {
        bail!(
            "could not obtain a token for '{}': run the vendor CLI login (az/gcloud), \
             or set an OAuth client id in this connection's settings",
            conn.name
        )
    }
}

/// Token-cache-aware credential resolution with a browser fallback. Order:
/// valid cached browser token -> vendor CLI -> browser prompt (GUI) or error
/// (headless). `has_gui_fallback` is false for MCP/CLI so a browser is never
/// requested there (those paths keep today's CLI-only behaviour).
pub fn resolve_password_with_cache(
    conn: &DbConnection,
    stored: Option<&str>,
    cached: Option<&CachedToken>,
    has_gui_fallback: bool,
) -> Result<PasswordResolution> {
    // Password auth and non-AAD/GCP token modes are unchanged: no browser path.
    if !matches!(conn.auth, DbAuth::AzureAd | DbAuth::GcpIam) {
        return resolve_password(conn, stored).map(PasswordResolution::Ready);
    }
    // 1. A valid cached browser token wins over any CLI shell-out.
    if let Some(t) = cached
        && token_still_valid(t, unix_now())
    {
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
struct AwsSsoConfig<'a> {
    start_url: &'a str,
    region: &'a str,
    account_id: &'a str,
    role: &'a str,
}

fn aws_sso_config(conn: &DbConnection) -> Option<AwsSsoConfig<'_>> {
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
struct AwsCreds {
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
}

/// Exchange the SSO access token for role credentials via the Identity Center
/// portal. Blocks on the network; call off the UI thread.
fn aws_role_credentials(conn: &DbConnection, sso_access_token: &str) -> Result<AwsCreds> {
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
mod tests {
    use super::*;
    use crate::db::DbEngine;

    fn conn(auth: DbAuth) -> DbConnection {
        DbConnection {
            id: "db-1".into(),
            name: "t".into(),
            engine: DbEngine::Postgres,
            host: "db.example.com".into(),
            port: 5432,
            database: "app".into(),
            username: "octa".into(),
            auth,
            allow_writes: false,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    #[test]
    fn browser_config_azure_pg_scope_and_endpoints() {
        let mut c = conn(DbAuth::AzureAd);
        c.oauth_client_id = Some("cid".into());
        c.oauth_tenant = Some("contoso.onmicrosoft.com".into());
        let cfg = browser_oauth_config(&c, None).expect("azure config");
        assert!(cfg.authorize_url.contains("contoso.onmicrosoft.com"));
        assert!(cfg.authorize_url.ends_with("/oauth2/v2.0/authorize"));
        assert!(cfg.token_url.ends_with("/oauth2/v2.0/token"));
        assert_eq!(
            cfg.scope,
            "https://ossrdbms-aad.database.windows.net/.default"
        );
        assert!(cfg.client_secret.is_none());
    }

    #[test]
    fn browser_config_gcp_uses_google_endpoints_and_secret() {
        let mut c = conn(DbAuth::GcpIam);
        c.oauth_client_id = Some("cid.apps.googleusercontent.com".into());
        let cfg = browser_oauth_config(&c, Some("gsecret")).expect("gcp config");
        assert_eq!(
            cfg.authorize_url,
            "https://accounts.google.com/o/oauth2/v2/auth"
        );
        assert_eq!(cfg.token_url, "https://oauth2.googleapis.com/token");
        assert_eq!(cfg.scope, "https://www.googleapis.com/auth/cloud-platform");
        assert_eq!(cfg.client_secret.as_deref(), Some("gsecret"));
    }

    #[test]
    fn browser_config_none_without_client_id() {
        let c = conn(DbAuth::AzureAd);
        assert!(browser_oauth_config(&c, None).is_none());
    }

    #[test]
    fn cached_valid_browser_token_is_used_before_cli() {
        let mut c = conn(DbAuth::AzureAd);
        c.oauth_client_id = Some("cid".into());
        let tok = CachedToken {
            access_token: "CACHED".into(),
            expires_at_unix: unix_now() + 3600,
        };
        let r = resolve_password_with_cache(&c, None, Some(&tok), true).unwrap();
        match r {
            PasswordResolution::Ready(t) => assert_eq!(t, "CACHED"),
            _ => panic!("expected the cached token"),
        }
    }

    #[test]
    fn cli_failure_with_client_id_and_gui_asks_for_browser() {
        let mut c = conn(DbAuth::AzureAd);
        c.oauth_client_id = Some("cid".into());
        let r = after_cli_failure(&c, true).unwrap();
        assert!(matches!(r, PasswordResolution::NeedsBrowserSignin));
    }

    #[test]
    fn cli_failure_with_client_id_headless_errors() {
        let mut c = conn(DbAuth::AzureAd);
        c.oauth_client_id = Some("cid".into());
        assert!(after_cli_failure(&c, false).is_err());
    }

    #[test]
    fn cli_failure_without_client_id_errors_with_hint() {
        let c = conn(DbAuth::AzureAd);
        let err = after_cli_failure(&c, true).unwrap_err().to_string();
        assert!(err.contains("OAuth client id"));
    }

    #[test]
    fn password_auth_has_no_command() {
        assert!(auth_command(&conn(DbAuth::Password)).is_none());
    }

    #[test]
    fn aws_iam_command_argv_with_region() {
        let c = conn(DbAuth::AwsIam {
            region: Some("eu-central-1".into()),
            sso_start_url: None,
            sso_region: None,
            sso_account_id: None,
            sso_role: None,
        });
        let (bin, args) = auth_command(&c).unwrap();
        assert_eq!(bin, "aws");
        assert_eq!(
            args,
            vec![
                "rds",
                "generate-db-auth-token",
                "--hostname",
                "db.example.com",
                "--port",
                "5432",
                "--username",
                "octa",
                "--region",
                "eu-central-1",
            ]
        );
    }

    #[test]
    fn aws_iam_command_argv_without_region() {
        let c = conn(DbAuth::AwsIam {
            region: None,
            sso_start_url: None,
            sso_region: None,
            sso_account_id: None,
            sso_role: None,
        });
        let (_, args) = auth_command(&c).unwrap();
        assert!(!args.contains(&"--region".to_string()));
    }

    #[test]
    fn aws_sso_config_needs_all_fields_and_falls_back_region() {
        // start URL alone is not enough: account id + role are required.
        let mut c = conn(DbAuth::AwsIam {
            region: Some("eu-west-1".into()),
            sso_start_url: Some("https://acme.awsapps.com/start".into()),
            sso_region: None,
            sso_account_id: None,
            sso_role: None,
        });
        assert!(!aws_sso_configured(&c));

        // Complete config: SSO region falls back to the DB region.
        c.auth = DbAuth::AwsIam {
            region: Some("eu-west-1".into()),
            sso_start_url: Some("https://acme.awsapps.com/start".into()),
            sso_region: None,
            sso_account_id: Some("123456789012".into()),
            sso_role: Some("DBReader".into()),
        };
        let sso = aws_sso_config(&c).expect("configured");
        assert_eq!(sso.region, "eu-west-1");
        assert_eq!(sso.account_id, "123456789012");
        assert_eq!(sso.role, "DBReader");

        // Explicit SSO region wins over the DB region.
        c.auth = DbAuth::AwsIam {
            region: Some("eu-west-1".into()),
            sso_start_url: Some("https://acme.awsapps.com/start".into()),
            sso_region: Some("us-east-1".into()),
            sso_account_id: Some("123456789012".into()),
            sso_role: Some("DBReader".into()),
        };
        assert_eq!(aws_sso_config(&c).unwrap().region, "us-east-1");
    }

    #[test]
    fn azure_ad_resource_switches_on_engine() {
        // Postgres / MySQL use the shared OSS-RDBMS resource...
        for engine in [DbEngine::Postgres, DbEngine::MySql] {
            let mut c = conn(DbAuth::AzureAd);
            c.engine = engine;
            let (bin, args) = auth_command(&c).unwrap();
            assert_eq!(bin, "az");
            assert!(
                args.contains(&"https://ossrdbms-aad.database.windows.net".to_string()),
                "{engine:?}: {args:?}"
            );
        }
        // ...SQL Server keeps its own.
        let mut c = conn(DbAuth::AzureAd);
        c.engine = DbEngine::Mssql;
        let (_, args) = auth_command(&c).unwrap();
        assert!(args.contains(&"https://database.windows.net/".to_string()));
    }

    #[test]
    fn gcp_iam_command_argv() {
        let (bin, args) = auth_command(&conn(DbAuth::GcpIam)).unwrap();
        assert_eq!(bin, "gcloud");
        assert_eq!(
            args,
            vec!["sql", "generate-login-token", "--format=value(token)"]
        );
    }

    #[test]
    fn password_resolution_uses_stored_secret() {
        let c = conn(DbAuth::Password);
        assert_eq!(resolve_password(&c, Some("pw")).unwrap(), "pw");
        let err = resolve_password(&c, None).unwrap_err().to_string();
        assert!(err.contains("Settings -> Databases"), "{err}");
        assert!(resolve_password(&c, Some("  ")).is_err());
    }

    // Throwaway 2048-bit RSA key generated for this test only (never a real
    // credential).
    const TEST_RSA_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC6LGcSVtbOGtq8\n\
sgo8L88+eQDPOpU9d5OuxnMxmc+nrGnmYV3HBnz2ujxWJeAtNjVe8pdiYhGWbIYG\n\
/+zNJIflTGTsEFa/IOh3HokcfniJKzUN0rzLubgGkOpu+99ToGYvVj1xXoAjB21D\n\
Ldmu9QoCHw+AOjWCYvH4j+cxyMCyQQqm4NtORBeSUlWSPQ/TAIX07zFNyAQbZcPd\n\
t00QB+oKfn2XzW8t8iZ2a4vPUGY9ysINF+aUo9WOAY4SjCaPpinosh6TOmvByXyy\n\
oMyLgaS0hh9mVUyhHu8VfTnHcWj2bSqmCrscnrERyAaoO1G6GjsHII4hJo2WePxB\n\
9o0cx+MnAgMBAAECggEAF9nQap0Nb+Io27vDa+qEFnDSFbpfnDxRgzaRU21tGQIR\n\
nx4iMXk3UTSSvkaj7abgN4XEtynxLuBAW202HSHs9wdOdp+xPVMt9PTIhAn/zzLl\n\
3Rt+bGsilFTEc+t4tPH7pVzbCkcdC1/MM6sQFEX4PkVUaw0KBeY/MaTd7ZbWeP97\n\
+ghamDI6GCDepxnzl+w1PJtdtwRs9hMD+MnkIHKlapW07L0X5quurWhrbqFdou9A\n\
k23Q6vN0QbXCSNQrlMkNMcBhpDM3lDbs3rJN+NSgz74VZN4NtgXGc2nJS9l9BKWK\n\
QED4WCaMqqxr2LMXycYD34PV/W9xHwrma46qWw3/AQKBgQDv7nwCWq3Y+GR+JwMI\n\
XyKfBUlVRKTwRxuNAcZdRE48LOls3baO5BacdTDN7DR6niJj6L4lH1CT1+c5xGPX\n\
nCcDjQQJhkEqKP9IjnfutyZv/jc1/+iql34h6iTfp6pfECF2GVTibw4fM6d2KVHl\n\
8xKGMqHRdId6kK5y7Yzr7t664QKBgQDGpEKOiZN91APU+wCf2ViPtTWVOX2yTgJw\n\
END0W+w8pYGQ9Q7OnHBqI6aIAi9tI5gpX07EulhIBJXgukN+XZXGSbQTsPzMxhVr\n\
82ZQvrCHorS3TiurjGNRVz6YpCTscQLtVGMjKcGyJoo6uMEojK1IHVDVRaQeaU/r\n\
UBxeacinBwKBgQCfgat9oS0sLk4Ys/THLwAEOe57umvwtUUyo/hs7skYJj90uZzx\n\
N990WlB8xchJsDFqvEMUyNG3x/QXqmE56LzjFm+VqWRPE/xLDzPaRnZOQ/IOezgB\n\
mT8oatHiwkN4eW/VZJfTBUOdKKju3B9vQP6Sqrx7U/3xVJg1HYnvG9mE4QKBgDkK\n\
Q/5wLQUt02feJ8C/BbpGe7t9BcYktnh9q7LvjnefwwPgCr9zlqgz0octsXayiOgg\n\
cRr2s5ECmBMvCWCI+RA2a9pXsVAP9WjJPPEkwMZCB4i3jry1FHPwDI6CXAP1M7T4\n\
zXr0a6V/gaP5F6ZQNxYFLQgA9m6xKPzWRT8rOy4TAoGABRb1/TxgKJEnD9QebF9g\n\
mN22xNhqgdEQXBAkKtsPWrTBZ0XFoNnqWQfmkYYMQ6cTclvI2DdL0LwZyW1EQCmA\n\
wltnxcqJb0IPM7DwwIRH2LDzkacFLd81RKUmYIvJDYmlBkhk9e/PW+a2yutmEXBj\n\
1h5u+blqA+zMYEXqbth0PgQ=\n\
-----END PRIVATE KEY-----\n";

    fn decode_claims_unverified(payload_b64: &str) -> serde_json::Value {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload_b64)
            .expect("base64url payload");
        serde_json::from_slice(&bytes).expect("claims json")
    }

    #[test]
    fn snowflake_jwt_has_iss_sub() {
        let jwt = snowflake_jwt("ORG-ACCT", "user", TEST_RSA_PEM.as_bytes(), None).unwrap();
        let parts: Vec<_> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims = decode_claims_unverified(parts[1]);
        assert_eq!(claims["sub"], "ORG-ACCT.USER"); // user uppercased
        let iss = claims["iss"].as_str().unwrap();
        assert!(iss.starts_with("ORG-ACCT.USER.SHA256:"), "{iss}");
        assert!(claims["exp"].as_u64().unwrap() > claims["iat"].as_u64().unwrap());
    }

    #[test]
    fn snowflake_jwt_rejects_encrypted() {
        let err = snowflake_jwt("A", "U", TEST_RSA_PEM.as_bytes(), Some("pw"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("encrypted"), "{err}");
    }

    fn sample_sa_key_json() -> serde_json::Value {
        serde_json::json!({
            "type": "service_account",
            "client_email": "svc@proj.iam.gserviceaccount.com",
            "private_key_id": "abc123",
            "token_uri": "https://oauth2.googleapis.com/token",
            "private_key": TEST_RSA_PEM,
        })
    }

    #[test]
    fn gcp_assertion_targets_token_uri_and_scope() {
        let key = sample_sa_key_json();
        let a = gcp_sa_assertion(&key, "https://www.googleapis.com/auth/bigquery").unwrap();
        let parts: Vec<_> = a.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims = decode_claims_unverified(parts[1]);
        assert_eq!(claims["scope"], "https://www.googleapis.com/auth/bigquery");
        assert_eq!(claims["aud"], key["token_uri"]);
        assert_eq!(claims["iss"], key["client_email"]);
        // The key id rides in the JWT header.
        let header = decode_claims_unverified(parts[0]);
        assert_eq!(header["kid"], "abc123");
    }

    #[test]
    fn gcp_assertion_errors_on_missing_field() {
        let err = gcp_sa_assertion(&serde_json::json!({}), "scope")
            .unwrap_err()
            .to_string();
        assert!(err.contains("client_email"), "{err}");
    }

    #[test]
    fn ephemeral_listener_reports_nonzero_port() {
        let (_l, port) = bind_ephemeral_listener().unwrap();
        assert!(port > 0);
    }

    #[test]
    fn token_parsed_from_redirect_request() {
        let raw = "GET /?token=abc%2B123&foo=bar HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(token_from_request(raw).as_deref(), Some("abc+123"));
        assert_eq!(token_from_request("GET / HTTP/1.1\r\n\r\n"), None);
    }

    #[test]
    fn percent_decode_basics() {
        assert_eq!(percent_decode("a%2Bb+c"), "a+b c");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn databricks_azure_resource() {
        assert_eq!(
            azure_resource_for(DbEngine::Databricks),
            "2ff814a6-3304-4ab8-85cb-cd0e6f879c1d"
        );
        assert_eq!(
            azure_resource_for(DbEngine::Mssql),
            "https://database.windows.net/"
        );
        assert_eq!(
            azure_resource_for(DbEngine::Postgres),
            "https://ossrdbms-aad.database.windows.net"
        );
    }

    #[test]
    fn token_valid_until_skew() {
        let t = CachedToken {
            access_token: "x".into(),
            expires_at_unix: 1000,
        };
        assert!(token_still_valid(&t, 900)); // valid
        assert!(!token_still_valid(&t, 995)); // within 60s skew -> expired
        assert!(!token_still_valid(&t, 1000)); // exactly at expiry -> expired
    }
}
