//! Generic OAuth 2.0 authorization-code flow with PKCE and a loopback redirect
//! (RFC 8252), shared by the DB and cloud browser sign-in fallbacks. Reuses the
//! token type and loopback machinery generalised out of `crate::db::auth`.
//!
//! `acquire_token` blocks on the browser round-trip; call it on a worker thread.

use anyhow::{Context, Result, bail};
use base64::Engine;
use sha2::{Digest, Sha256};

/// Everything provider-specific about one browser sign-in. The flow itself is
/// identical across Azure and Google; only these values differ.
#[derive(Debug, Clone)]
pub struct OAuthBrowserConfig {
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    /// Google desktop clients require a (non-confidential) secret in the token
    /// exchange; Azure public clients must NOT send one. None = omit.
    pub client_secret: Option<String>,
    pub scope: String,
    /// Extra query params on the authorize URL (e.g. Google `access_type`).
    pub extra_auth_params: Vec<(String, String)>,
}

/// A minted access token plus its absolute expiry (unix seconds).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedToken {
    pub access_token: String,
    pub expires_at_unix: u64,
}

/// Whether a cached token is still usable at `now_unix`, keeping a 60-second
/// skew margin so a token about to expire is treated as expired.
pub fn token_still_valid(t: &CachedToken, now_unix: u64) -> bool {
    now_unix.saturating_add(60) < t.expires_at_unix
}

/// Current unix time in seconds.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Best-effort open of a URL in the system browser (for the sign-in flow).
pub fn open_url_in_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let (bin, args): (&str, &[&str]) = ("xdg-open", &[]);
    #[cfg(target_os = "macos")]
    let (bin, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "windows")]
    let (bin, args): (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
    let _ = std::process::Command::new(bin).args(args).arg(url).spawn();
}

/// Generate a PKCE (verifier, challenge) pair. Verifier is 32 random bytes
/// base64url-encoded (43 chars); challenge is base64url(SHA-256(verifier)),
/// both without padding, per RFC 7636.
pub fn pkce_pair() -> (String, String) {
    let bytes: [u8; 32] = rand::random();
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

/// Percent-encode a value for a query string: everything outside the unreserved
/// set (`A-Za-z0-9-._~`) becomes `%XX`.
fn encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the authorize URL for the loopback redirect flow.
pub fn build_authorize_url(
    cfg: &OAuthBrowserConfig,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> String {
    let mut params = vec![
        ("response_type".to_string(), "code".to_string()),
        ("client_id".to_string(), cfg.client_id.clone()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("scope".to_string(), cfg.scope.clone()),
        ("code_challenge".to_string(), challenge.to_string()),
        ("code_challenge_method".to_string(), "S256".to_string()),
        ("state".to_string(), state.to_string()),
    ];
    params.extend(cfg.extra_auth_params.iter().cloned());
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{}", cfg.authorize_url, query)
}

/// Bind a loopback TCP listener on an OS-assigned port used as the browser
/// redirect target.
pub(crate) fn bind_ephemeral_listener() -> Result<(std::net::TcpListener, u16)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .context("binding a loopback port for the browser redirect")?;
    let port = listener
        .local_addr()
        .context("reading the redirect listener port")?
        .port();
    Ok((listener, port))
}

/// Percent-decode an `application/x-www-form-urlencoded` value (`+` is a space,
/// `%XX` a byte). Best-effort: undecodable escapes pass through.
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extract `(code, state)` from the raw HTTP redirect request-line query.
/// Returns None when the provider redirected with an `error` (no `code`).
fn code_from_redirect(raw: &str) -> Option<(String, String)> {
    let mut code = None;
    let mut state = None;
    for part in raw.split(['?', '&', '\n', '\r', ' ']) {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("code=") {
            code = Some(percent_decode(v));
        } else if let Some(v) = part.strip_prefix("state=") {
            state = Some(percent_decode(v));
        }
    }
    Some((code?, state.unwrap_or_default()))
}

/// Parse the token endpoint JSON into a `CachedToken`. Surfaces the provider's
/// `error_description` (or `error`) verbatim on an OAuth error payload.
fn parse_token_response(text: &str) -> Result<CachedToken> {
    let v: serde_json::Value =
        serde_json::from_str(text).context("parsing the OAuth token response")?;
    if let Some(desc) = v
        .get("error_description")
        .or_else(|| v.get("error"))
        .and_then(|e| e.as_str())
    {
        bail!("OAuth token endpoint error: {desc}");
    }
    let access_token = v
        .get("access_token")
        .and_then(|t| t.as_str())
        .context("OAuth token response had no access_token")?
        .to_string();
    let expires_in = v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(3600);
    Ok(CachedToken {
        access_token,
        expires_at_unix: unix_now().saturating_add(expires_in),
    })
}

/// Run the full browser sign-in: PKCE, loopback, open the browser, catch the
/// redirect, exchange the code for an access token.
///
/// Blocks on the browser round-trip and the network; call off the UI thread.
pub fn acquire_token(cfg: &OAuthBrowserConfig, open_browser: impl Fn(&str)) -> Result<CachedToken> {
    use std::io::{Read, Write};

    let (verifier, challenge) = pkce_pair();
    let (state, _) = pkce_pair(); // reuse the random generator for an opaque state
    let (listener, port) = bind_ephemeral_listener()?;
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let url = build_authorize_url(cfg, &redirect_uri, &challenge, &state);
    open_browser(&url);

    let (mut stream, _) = listener
        .accept()
        .context("waiting for the browser redirect")?;
    let mut buf = [0u8; 8192];
    let n = stream
        .read(&mut buf)
        .context("reading the browser redirect")?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let (code, got_state) = code_from_redirect(&raw)
        .context("the browser redirect carried no authorization code (sign-in cancelled?)")?;
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
          <html><body>Sign-in complete. You may close this tab.</body></html>",
    );

    // Security: the redirect's state MUST match what we sent.
    if got_state != state {
        bail!("browser redirect state mismatch (possible CSRF); sign-in rejected");
    }

    // Exchange the code (+ PKCE verifier) for tokens.
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &cfg.client_id),
        ("code_verifier", &verifier),
    ];
    if let Some(secret) = cfg.client_secret.as_deref().filter(|s| !s.is_empty()) {
        form.push(("client_secret", secret));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .post(&cfg.token_url)
        .send_form(form)
        .context("posting the OAuth token exchange")?;
    let text = resp
        .body_mut()
        .read_to_string()
        .context("reading the OAuth token response")?;
    parse_token_response(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_base64url_sha256_of_verifier() {
        let (verifier, challenge) = pkce_pair();
        assert!(verifier.len() >= 43);
        assert!(!challenge.contains('='));
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        let digest = Sha256::digest(verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(challenge, expected);
    }

    #[test]
    fn authorize_url_has_all_required_params() {
        let cfg = OAuthBrowserConfig {
            authorize_url: "https://login.example.com/authorize".into(),
            token_url: "https://login.example.com/token".into(),
            client_id: "cid-123".into(),
            client_secret: None,
            scope: "https://storage.example.com/.default".into(),
            extra_auth_params: vec![("access_type".into(), "offline".into())],
        };
        let url = build_authorize_url(&cfg, "http://127.0.0.1:5000", "CHAL", "STATE");
        assert!(url.starts_with("https://login.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=cid-123"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000"));
        assert!(url.contains("code_challenge=CHAL"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=STATE"));
        assert!(url.contains("scope=https%3A%2F%2Fstorage.example.com%2F.default"));
        assert!(url.contains("access_type=offline"));
    }

    #[test]
    fn code_and_state_extracted_from_redirect_request() {
        let raw = "GET /?code=AUTHCODE123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let (code, state) = code_from_redirect(raw).expect("should parse");
        assert_eq!(code, "AUTHCODE123");
        assert_eq!(state, "xyz");
    }

    #[test]
    fn redirect_error_yields_no_code() {
        let raw = "GET /?error=access_denied&state=xyz HTTP/1.1\r\n\r\n";
        assert!(code_from_redirect(raw).is_none());
    }

    #[test]
    fn token_validity_keeps_60s_skew() {
        let t = CachedToken {
            access_token: "x".into(),
            expires_at_unix: 1000,
        };
        assert!(token_still_valid(&t, 900));
        assert!(!token_still_valid(&t, 950));
    }

    #[test]
    fn token_response_parsed_with_expiry() {
        let json = r#"{"access_token":"AT-999","expires_in":3600,"token_type":"Bearer"}"#;
        let before = unix_now();
        let t = parse_token_response(json).expect("parse");
        assert_eq!(t.access_token, "AT-999");
        assert!(t.expires_at_unix >= before + 3600);
        assert!(t.expires_at_unix <= unix_now() + 3600 + 5);
    }

    #[test]
    fn token_response_defaults_expiry_when_missing() {
        let json = r#"{"access_token":"AT-1"}"#;
        let t = parse_token_response(json).expect("parse");
        assert!(t.expires_at_unix > unix_now());
    }

    #[test]
    fn token_response_error_surfaces_message() {
        let json = r#"{"error":"invalid_client","error_description":"bad client id"}"#;
        let err = parse_token_response(json).unwrap_err().to_string();
        assert!(err.contains("bad client id"));
    }
}
