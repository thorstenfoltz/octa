//! End-to-end exercise of the browser sign-in loop, with a fake browser and a
//! fake token endpoint standing in for Google.
//!
//! These exist because three separate sign-in defects shipped while the pure
//! helpers around this function were all green: the parts with tests were not
//! the parts that broke. The one that matters most is
//! [`a_provider_that_never_redirects_times_out`] - a provider refusing
//! *before* its consent screen shows its own error page and never contacts
//! the loopback listener at all, which used to block the worker thread for
//! the life of the process and leave the Sign in button spinning forever.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use octa::auth::oauth_browser::{OAuthBrowserConfig, acquire_token_cancellable};

/// A one-shot HTTP server that answers a single request with `body`, then
/// stops. Returns its URL and the thread handle.
fn fake_token_endpoint(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind token endpoint");
    let url = format!("http://{}/", listener.local_addr().expect("addr"));
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    (url, handle)
}

/// Pull a query parameter out of the authorize URL the code under test built.
fn param(url: &str, key: &str) -> String {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix(&format!("{key}=")))
        .map(|v| v.replace("%3A", ":").replace("%2F", "/"))
        .unwrap_or_else(|| panic!("no {key} in {url}"))
}

/// Play the browser: read the authorize URL, then hit the loopback redirect
/// with `query`, exactly as a real browser would after the consent screen.
fn redirect_with(url: &str, query: String) {
    let redirect = param(url, "redirect_uri");
    let addr = redirect.trim_start_matches("http://").trim_end_matches('/');
    // The listener is polled, so a connection may race the first accept;
    // retry briefly rather than flake.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut s) = TcpStream::connect(addr) {
            let _ = s
                .write_all(format!("GET /?{query} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes());
            let mut sink = Vec::new();
            let _ = s.read_to_end(&mut sink);
            return;
        }
        assert!(Instant::now() < deadline, "never reached {addr}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn config(token_url: String) -> OAuthBrowserConfig {
    OAuthBrowserConfig {
        authorize_url: "http://127.0.0.1:1/authorize".to_string(),
        token_url,
        client_id: "test-client".to_string(),
        client_secret: None,
        scope: "https://www.googleapis.com/auth/spreadsheets.readonly".to_string(),
        extra_auth_params: vec![("access_type".to_string(), "offline".to_string())],
    }
}

#[test]
fn a_completed_sign_in_returns_the_token() {
    let (token_url, server) =
        fake_token_endpoint(r#"{"access_token":"tok-123","expires_in":3600}"#);
    let never = AtomicBool::new(false);
    let got = acquire_token_cancellable(
        &config(token_url),
        |url| {
            let state = param(url, "state");
            let url = url.to_string();
            std::thread::spawn(move || redirect_with(&url, format!("code=abc&state={state}")));
        },
        &never,
        Duration::from_secs(10),
    )
    .expect("sign-in should succeed");
    assert_eq!(got.access_token, "tok-123");
    assert!(got.expires_at_unix > 0);
    let _ = server.join();
}

/// The regression test for the bug that started all this: Google refuses
/// before the consent screen, so nothing ever arrives. The wait must end.
///
/// Run on a worker and collected with a receive timeout, deliberately: if the
/// unbounded `accept()` ever comes back, calling it inline would wedge the
/// whole test binary instead of reporting a failure, and a hung CI job is a
/// far worse bug report than a red one.
#[test]
fn a_provider_that_never_redirects_times_out() {
    let (token_url, _server) = fake_token_endpoint("{}");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let never = AtomicBool::new(false);
        let out = acquire_token_cancellable(
            &config(token_url),
            |_url| { /* a browser that shows an error page and never comes back */ },
            &never,
            Duration::from_millis(400),
        );
        let _ = tx.send(out.map(|_| ()).map_err(|e| format!("{e:#}")));
    });
    let outcome = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the sign-in wait never ended: it is unbounded again");
    let msg = outcome.expect_err("an unanswered sign-in must not succeed");
    assert!(msg.contains("never came back"), "{msg}");
}

/// What the Cancel button does. It must not wait out the timeout.
#[test]
fn cancelling_abandons_the_wait() {
    let (token_url, _server) = fake_token_endpoint("{}");
    let cancel = AtomicBool::new(true);
    let started = Instant::now();
    let err = acquire_token_cancellable(
        &config(token_url),
        |_url| {},
        &cancel,
        // Far longer than the test may take: passing it would mean cancel was
        // ignored and the deadline did the work instead.
        Duration::from_secs(120),
    )
    .expect_err("a cancelled sign-in must not succeed");
    assert!(started.elapsed() < Duration::from_secs(5), "cancel ignored");
    assert!(format!("{err:#}").contains("cancelled"), "{err:#}");
}

/// A refusal at the consent screen does come back, and must be reported in
/// the provider's own words rather than guessed at as a cancellation.
#[test]
fn a_refusal_redirect_reports_the_providers_reason() {
    let (token_url, _server) = fake_token_endpoint("{}");
    let never = AtomicBool::new(false);
    let err = acquire_token_cancellable(
        &config(token_url),
        |url| {
            let url = url.to_string();
            std::thread::spawn(move || {
                redirect_with(
                    &url,
                    "error=admin_policy_enforced&error_description=Blocked+by+admin".to_string(),
                )
            });
        },
        &never,
        Duration::from_secs(10),
    )
    .expect_err("a refused sign-in must not succeed");
    let msg = format!("{err:#}");
    assert!(msg.contains("admin_policy_enforced"), "{msg}");
    assert!(msg.contains("Blocked by admin"), "{msg}");
}

/// The CSRF guard: a redirect whose `state` is not the one we sent is
/// rejected even though it carries a usable-looking code.
#[test]
fn a_mismatched_state_is_rejected() {
    let (token_url, _server) = fake_token_endpoint(r#"{"access_token":"tok","expires_in":60}"#);
    let never = AtomicBool::new(false);
    let err = acquire_token_cancellable(
        &config(token_url),
        |url| {
            let url = url.to_string();
            std::thread::spawn(move || {
                redirect_with(&url, "code=abc&state=not-the-one-we-sent".to_string())
            });
        },
        &never,
        Duration::from_secs(10),
    )
    .expect_err("a state mismatch must be rejected");
    assert!(format!("{err:#}").contains("state mismatch"), "{err:#}");
}
