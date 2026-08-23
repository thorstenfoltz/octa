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

/// True for a plain HTTP(S) URL.
///
/// Cloud schemes (`s3://`, `az://`, `gs://`) are handled by
/// [`parse_cloud_url`] and must not match here: they need credentials and a
/// provider, this needs neither.
pub fn is_http_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Extension of the URL's path component, lowercased, ignoring any query or
/// fragment. `None` when the last path segment has no dot, which leaves the
/// caller to fall back to content sniffing.
fn url_extension(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    let path = after_scheme.split(['?', '#']).next()?;
    let last = path.rsplit('/').next()?;
    let ext = last.rsplit_once('.')?.1;
    if ext.is_empty() {
        None
    } else {
        Some(ext.to_lowercase())
    }
}

/// Who chose the URL, and therefore what it may point at.
///
/// A person typing an address has authorised that request; an agent choosing
/// one from data it just read has not. The distinction matters because the
/// interesting targets are all reachable only from this machine: a cloud
/// metadata endpoint (169.254.169.254) hands out credentials, and an intranet
/// service is reachable here and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlTrust {
    /// A human typed or pasted it (CLI argument, GUI dialog). No restriction:
    /// reading from `localhost:8000` is a normal thing to ask for.
    UserSupplied,
    /// An agent or tool argument chose it (MCP, chat). Confined to globally
    /// routable addresses.
    AgentSupplied,
}

/// Whether an address is globally routable, i.e. not something only this
/// machine or this network can reach.
///
/// `Ipv6Addr::is_unique_local` and friends are still unstable, so the ranges
/// are spelled out.
fn is_global_address(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 100.64.0.0/10, carrier-grade NAT
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
                // 0.0.0.0/8 and 240.0.0.0/4
                || v4.octets()[0] == 0
                || v4.octets()[0] >= 240)
        }
        IpAddr::V6(v6) => {
            let o = v6.octets();
            !(v6.is_loopback()
                || v6.is_unspecified()
                // fc00::/7 unique local
                || (o[0] & 0xfe) == 0xfc
                // fe80::/10 link local
                || (o[0] == 0xfe && (o[1] & 0xc0) == 0x80)
                // ::ffff:0:0/96 IPv4-mapped: judge the mapped address instead
                || v6.to_ipv4_mapped().is_some_and(|v4| !is_global_address(IpAddr::V4(v4))))
        }
    }
}

/// Refuse a URL an agent should not be able to reach.
///
/// Resolves the host and rejects the request if *any* resolved address is
/// non-global, so a name that resolves to both a public and a private address
/// does not slip through.
///
/// Ceiling, stated rather than pretended away: this resolves and then lets
/// ureq resolve again, so a name that changes answers between the two calls
/// (DNS rebinding) is not covered. Closing that needs connecting to the
/// address we validated, which ureq does not expose.
fn ensure_agent_may_fetch(url: &str) -> Result<()> {
    use std::net::ToSocketAddrs;

    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // Strip userinfo, then split host and optional port (IPv6 in brackets).
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        let (h, tail) = rest
            .split_once(']')
            .ok_or_else(|| anyhow::anyhow!("malformed IPv6 host in {url}"))?;
        (
            h.to_string(),
            tail.strip_prefix(':').unwrap_or("").to_string(),
        )
    } else {
        match hostport.split_once(':') {
            Some((h, p)) => (h.to_string(), p.to_string()),
            None => (hostport.to_string(), String::new()),
        }
    };
    if host.is_empty() {
        anyhow::bail!("no host in {url}");
    }
    let port: u16 = port
        .parse()
        .unwrap_or(if url.starts_with("https://") { 443 } else { 80 });

    let addrs: Vec<std::net::SocketAddr> = (host.as_str(), port)
        .to_socket_addrs()
        .with_context(|| format!("resolving {host}"))?
        .collect();
    if addrs.is_empty() {
        anyhow::bail!("{host} did not resolve");
    }
    for addr in &addrs {
        if !is_global_address(addr.ip()) {
            anyhow::bail!(
                "refusing to fetch {url}: {host} resolves to {}, which is not a public address. \
                 Tool-supplied URLs are limited to public hosts so that a document cannot \
                 talk this server into reading a cloud metadata endpoint or an internal \
                 service. Open it yourself from the command line or File > Open URL if you \
                 meant to.",
                addr.ip()
            );
        }
    }
    Ok(())
}

/// What a download produced.
pub struct FetchOutcome {
    pub path: PathBuf,
    /// The address actually fetched, when redirects took it somewhere other
    /// than the one asked for. `None` when it went straight there.
    pub redirected_to: Option<String>,
}

/// How many redirects to follow before calling it a loop.
const MAX_REDIRECTS: usize = 10;

/// Resolve a `Location` header against the URL it came from.
///
/// Servers are allowed to send a relative location, so `/b.csv` from
/// `http://h/a/x.csv` has to become `http://h/b.csv` rather than being
/// treated as a path on disk.
fn resolve_location(base: &str, location: &str) -> Option<String> {
    if is_http_url(location) {
        return Some(location.to_string());
    }
    let (scheme, rest) = base.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if let Some(abs) = location.strip_prefix('/') {
        return Some(format!("{scheme}://{authority}/{abs}"));
    }
    // Relative to the current directory.
    let path = &rest[authority.len()..];
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    Some(format!("{scheme}://{authority}{dir}{location}"))
}

/// Download an HTTP(S) URL to a temp file.
///
/// Redirects are followed by hand rather than by the HTTP library, for two
/// reasons: every hop is checked in its own right (a check on only the first
/// address is exactly what a redirect defeats), and the caller learns where it
/// actually ended up, so it can ask the user before opening something from an
/// address they did not name.
///
/// The temp keeps the URL's extension so the registry routes it to the right
/// reader; without one the caller sniffs the content. Leaked with `keep()` for
/// the same reason as [`fetch_url_to_temp`]: streaming readers keep reading
/// after this returns.
///
/// Read-only and unauthenticated by design. Anything needing credentials is
/// what the cloud connections are for.
pub fn fetch_http_to_temp(url: &str, trust: UrlTrust) -> Result<FetchOutcome> {
    // No automatic redirects: the loop below owns them so each hop is checked.
    let agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .build()
        .new_agent();

    let mut current = url.to_string();
    let mut hops = 0usize;
    // Did the address the user actually named point at the public internet?
    // If so, a redirect inward is a surprise worth refusing even on the
    // user-supplied path: they asked for a public host and something tried to
    // send them into their own network. If they named a private address to
    // begin with (a local dev server), redirects within it are their business.
    let started_public = trust == UrlTrust::AgentSupplied || ensure_agent_may_fetch(url).is_ok();
    let mut resp = loop {
        if trust == UrlTrust::AgentSupplied || (started_public && hops > 0) {
            ensure_agent_may_fetch(&current).with_context(|| {
                if hops > 0 {
                    format!("{url} redirected out of the public internet")
                } else {
                    String::new()
                }
            })?;
        }
        // A 404 HTML page parsed as a one-column table is the failure mode
        // this guards against, so a non-2xx status is an error naming the
        // code, never a file. The message has to stand alone: the CLI prints
        // `{e}` without the cause chain.
        let resp = match agent.get(&current).call() {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(code)) => {
                anyhow::bail!("{current} returned HTTP {code}");
            }
            Err(e) => {
                return Err(anyhow::Error::new(e).context(format!("downloading {current}")));
            }
        };

        if !resp.status().is_redirection() {
            break resp;
        }
        let Some(location) = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
        else {
            anyhow::bail!(
                "{current} returned HTTP {} with no Location",
                resp.status().as_u16()
            );
        };
        let next = resolve_location(&current, &location).ok_or_else(|| {
            anyhow::anyhow!("{current} redirected to an address Octa cannot read: {location}")
        })?;
        // Only http(s): a redirect to another scheme is never something to
        // follow.
        if !is_http_url(&next) {
            anyhow::bail!("{current} redirected to a non-web address: {next}");
        }
        hops += 1;
        if hops > MAX_REDIRECTS {
            anyhow::bail!("{url} redirected more than {MAX_REDIRECTS} times; giving up");
        }
        current = next;
    };

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("{current} returned HTTP {}", status.as_u16());
    }

    let ext = url_extension(&current)
        .or_else(|| url_extension(url))
        .unwrap_or_else(|| "bin".to_string());
    let mut tmp = tempfile::Builder::new()
        .prefix("octa-http-")
        .suffix(&format!(".{ext}"))
        .tempfile()?;
    std::io::copy(&mut resp.body_mut().as_reader(), &mut tmp)
        .with_context(|| format!("saving {current}"))?;
    let path = tmp.path().to_path_buf();
    let _ = tmp.keep();
    Ok(FetchOutcome {
        path,
        redirected_to: (current != url).then_some(current),
    })
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
}
