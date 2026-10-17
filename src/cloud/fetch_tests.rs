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

#[test]
fn recognises_http_urls_only() {
    assert!(is_http_url("https://example.com/a.csv"));
    assert!(is_http_url("http://example.com/a.csv"));
    // Cloud schemes have their own resolver and must not match here.
    assert!(!is_http_url("s3://bucket/a.csv"));
    assert!(!is_http_url("az://container/a.csv"));
    assert!(!is_http_url("gs://bucket/a.csv"));
    // Local paths, including a Windows drive letter.
    assert!(!is_http_url("/tmp/a.csv"));
    assert!(!is_http_url("C:\\data\\a.csv"));
    assert!(!is_http_url("a.csv"));
}

#[test]
fn extension_comes_from_the_path_not_the_query() {
    assert_eq!(url_extension("https://h/d/a.csv"), Some("csv".to_string()));
    // A signed URL carries a query string; the extension is still csv.
    assert_eq!(
        url_extension("https://h/d/a.csv?token=abc&x=1"),
        Some("csv".to_string())
    );
    assert_eq!(
        url_extension("https://h/d/a.parquet#frag"),
        Some("parquet".to_string())
    );
    // Upper case normalises, so reader lookup by extension still matches.
    assert_eq!(url_extension("https://h/A.CSV"), Some("csv".to_string()));
    // No extension at all: the caller falls back to content sniffing.
    assert_eq!(url_extension("https://h/d/data"), None);
    assert_eq!(url_extension("https://h/"), None);
    // A dot in a directory must not be mistaken for the file's extension.
    assert_eq!(url_extension("https://h/v1.2/data"), None);
}

#[test]
fn private_and_loopback_addresses_are_not_global() {
    use std::net::IpAddr;
    let not_global = [
        "127.0.0.1",       // loopback
        "10.1.2.3",        // private
        "172.16.0.1",      // private
        "192.168.1.1",     // private
        "169.254.169.254", // link-local: the cloud metadata endpoint
        "0.0.0.0",
        "100.64.0.1", // carrier-grade NAT
        "::1",
        "fe80::1",                // link-local
        "fc00::1",                // unique local
        "::ffff:127.0.0.1",       // IPv4-mapped loopback
        "::ffff:169.254.169.254", // IPv4-mapped metadata endpoint
    ];
    for s in not_global {
        let ip: IpAddr = s.parse().unwrap();
        assert!(!is_global_address(ip), "{s} must not count as global");
    }

    let global = ["1.1.1.1", "93.184.216.34", "2606:4700:4700::1111"];
    for s in global {
        let ip: IpAddr = s.parse().unwrap();
        assert!(is_global_address(ip), "{s} should count as global");
    }
}

#[test]
fn an_agent_cannot_fetch_a_loopback_url() {
    // The exact shape of a prompt-injection SSRF: a document names a URL,
    // the agent obligingly opens it.
    let err = ensure_agent_may_fetch("http://127.0.0.1:8000/secret.csv")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a public address"), "{err}");

    let err = ensure_agent_may_fetch("http://169.254.169.254/latest/meta-data/")
        .unwrap_err()
        .to_string();
    assert!(err.contains("refusing to fetch"), "{err}");

    // Userinfo must not smuggle a different host past the parser.
    let err = ensure_agent_may_fetch("http://example.com@127.0.0.1/x.csv")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a public address"), "{err}");

    // Bracketed IPv6 with a port.
    let err = ensure_agent_may_fetch("http://[::1]:9000/x.csv")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a public address"), "{err}");
}

#[test]
fn relative_redirect_targets_resolve_against_the_current_url() {
    // Absolute stays as it is.
    assert_eq!(
        resolve_location("http://h/a/x.csv", "https://other/y.csv").as_deref(),
        Some("https://other/y.csv")
    );
    // Root-relative replaces the whole path.
    assert_eq!(
        resolve_location("http://h/a/x.csv", "/b.csv").as_deref(),
        Some("http://h/b.csv")
    );
    // Directory-relative keeps the current directory.
    assert_eq!(
        resolve_location("http://h/a/x.csv", "b.csv").as_deref(),
        Some("http://h/a/b.csv")
    );
    // A host with no path at all.
    assert_eq!(
        resolve_location("http://h", "/b.csv").as_deref(),
        Some("http://h/b.csv")
    );
}

#[test]
fn a_relative_redirect_cannot_change_host() {
    // The point of resolving rather than trusting: a relative location
    // stays on the host that sent it, so it is still the checked host.
    let got = resolve_location("http://public.example/a", "/../../etc/passwd").unwrap();
    assert!(got.starts_with("http://public.example/"), "{got}");
}
