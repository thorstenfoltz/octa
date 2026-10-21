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
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: super::super::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
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
fn cli_failure_without_client_id_still_offers_the_browser() {
    // The point of the whole change: no client id is needed to be offered a
    // sign-in, because the vendor CLI's own login does the browser trip.
    let c = conn(DbAuth::AzureAd);
    let r = after_cli_failure(&c, true).unwrap();
    assert!(matches!(r, PasswordResolution::NeedsBrowserSignin));
}

#[test]
fn cli_failure_headless_names_the_command_to_run() {
    // No GUI to open a browser from (CLI / MCP), so the error has to be
    // something the reader can paste into a terminal.
    let c = conn(DbAuth::AzureAd);
    let err = after_cli_failure(&c, false).unwrap_err().to_string();
    assert!(err.contains("az login"), "{err}");

    let g = conn(DbAuth::GcpIam);
    let err = after_cli_failure(&g, false).unwrap_err().to_string();
    assert!(err.contains("gcloud auth login"), "{err}");
}

#[test]
fn login_commands_are_the_interactive_ones() {
    // `auth_command` mints a token and fails when signed out; these are the
    // ones that put the user in front of a browser. Mixing them up would
    // leave sign-in silently doing nothing.
    assert_eq!(
        cli_login_command(&conn(DbAuth::AzureAd)).map(|(b, a)| (b, a.join(" "))),
        Some(("az", "login".to_string()))
    );
    assert_eq!(
        cli_login_command(&conn(DbAuth::GcpIam)).map(|(b, a)| (b, a.join(" "))),
        Some(("gcloud", "auth login".to_string()))
    );
    assert!(cli_login_command(&conn(DbAuth::Password)).is_none());
}

#[test]
fn a_missing_cli_says_how_to_install_it_and_how_to_avoid_it() {
    // The message a user actually hits when they have no az installed. It
    // has to name the tool, an install command for this OS, and the way out
    // that needs no tool at all.
    let msg = cli_missing_message("az");
    assert!(msg.contains("az"), "{msg}");
    assert!(msg.contains(cli_install_command("az")), "{msg}");
    assert!(!cli_install_command("az").is_empty());
    // Every vendor CLI we shell out to has install guidance on every OS.
    for bin in ["az", "gcloud", "aws"] {
        assert!(!cli_install_command(bin).is_empty(), "{bin}");
    }
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

/// A cached browser token is what every connect path reads, and Sign out
/// forgets it. (The keyring copy is skipped under `cfg(test)`, so this runs
/// on the in-process cache only and never touches the real keyring.)
#[test]
fn a_cached_browser_token_is_used_until_signed_out() {
    let id = "test-token-cache-roundtrip";
    let tok = CachedToken {
        access_token: "BROWSER".into(),
        expires_at_unix: unix_now() + 3600,
    };
    cache_browser_token(id, tok);
    assert_eq!(
        cached_browser_token(id).map(|t| t.access_token).as_deref(),
        Some("BROWSER")
    );
    let mut c = conn(DbAuth::AzureAd);
    c.id = id.into();
    assert_eq!(resolve_password(&c, None).unwrap(), "BROWSER");
    forget_browser_token(id);
    assert!(cached_browser_token(id).is_none());
}

/// An expired token is never handed out, even though it is still cached.
#[test]
fn an_expired_browser_token_is_not_used() {
    let id = "test-token-cache-expired";
    cache_browser_token(
        id,
        CachedToken {
            access_token: "OLD".into(),
            expires_at_unix: unix_now().saturating_sub(10),
        },
    );
    assert!(cached_browser_token(id).is_none());
}

fn aws_sso_conn() -> DbConnection {
    conn(DbAuth::AwsIam {
        region: Some("eu-west-1".into()),
        sso_start_url: Some("https://acme.awsapps.com/start".into()),
        sso_region: None,
        sso_account_id: Some("123456789012".into()),
        sso_role: Some("ReadOnly".into()),
    })
}

/// Identity Center has no CLI step: with no valid sign-in the GUI is told to
/// offer the browser, the command line gets a sentence saying where.
#[test]
fn aws_identity_center_without_a_token_asks_for_the_browser() {
    let c = aws_sso_conn();
    assert!(browser_signin_offered(&c));
    assert!(matches!(
        resolve_password_with_cache(&c, None, None, true).unwrap(),
        PasswordResolution::NeedsBrowserSignin
    ));
    let err = resolve_password_with_cache(&c, None, None, false).unwrap_err();
    assert!(format!("{err:#}").contains("Settings -> Databases"));
}

/// Databricks browser OAuth takes the cached token as its bearer, and asks
/// for the browser once it has expired.
#[test]
fn databricks_browser_oauth_uses_or_asks_for_the_token() {
    let mut c = conn(DbAuth::OAuthBrowser);
    c.engine = DbEngine::Databricks;
    assert!(browser_signin_offered(&c));
    let fresh = CachedToken {
        access_token: "BEARER".into(),
        expires_at_unix: unix_now() + 3600,
    };
    match resolve_password_with_cache(&c, None, Some(&fresh), true).unwrap() {
        PasswordResolution::Ready(t) => assert_eq!(t, "BEARER"),
        _ => panic!("expected the cached token"),
    }
    let stale = CachedToken {
        access_token: "OLD".into(),
        expires_at_unix: unix_now().saturating_sub(10),
    };
    assert!(matches!(
        resolve_password_with_cache(&c, None, Some(&stale), true).unwrap(),
        PasswordResolution::NeedsBrowserSignin
    ));
}

/// A password connection never offers a browser.
#[test]
fn a_password_connection_offers_no_browser_sign_in() {
    assert!(!browser_signin_offered(&conn(DbAuth::Password)));
}

/// Snowflake's external-browser sign-in is offered in the sidebar too, takes
/// a cached token as its bearer and asks for the browser without one.
#[test]
fn snowflake_browser_sso_uses_or_asks_for_the_token() {
    let mut c = conn(DbAuth::OAuthBrowser);
    c.engine = DbEngine::Snowflake;
    assert!(browser_signin_offered(&c));
    let fresh = CachedToken {
        access_token: "SESSION".into(),
        expires_at_unix: unix_now() + 3600,
    };
    match resolve_password_with_cache(&c, None, Some(&fresh), true).unwrap() {
        PasswordResolution::Ready(t) => assert_eq!(t, "SESSION"),
        _ => panic!("expected the cached token"),
    }
    assert!(matches!(
        resolve_password_with_cache(&c, None, None, true).unwrap(),
        PasswordResolution::NeedsBrowserSignin
    ));
}
