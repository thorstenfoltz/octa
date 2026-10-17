/// A provider that refuses at the consent screen redirects back with an
/// `error`, and saying "cancelled?" there sends the user looking for a
/// mistake they did not make.
#[test]
fn a_refusal_redirect_reports_the_provider_reason() {
    let raw = "GET /?error=access_denied&error_description=The+user+denied+access HTTP/1.1";
    assert_eq!(
        error_from_redirect(raw).as_deref(),
        Some("access_denied: The user denied access")
    );
}

#[test]
fn an_error_without_a_description_still_names_itself() {
    let raw = "GET /?error=admin_policy_enforced HTTP/1.1";
    assert_eq!(
        error_from_redirect(raw).as_deref(),
        Some("admin_policy_enforced")
    );
}

/// A successful redirect must not be read as an error.
#[test]
fn a_successful_redirect_carries_no_error() {
    let raw = "GET /?code=abc&state=xyz HTTP/1.1";
    assert_eq!(error_from_redirect(raw), None);
}

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
