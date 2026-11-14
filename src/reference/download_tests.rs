use super::*;
use crate::reference::test_server::{not_modified, ok, serve, status};

#[test]
fn a_download_keeps_its_validators_and_sends_only_the_user_agent() {
    let (base, server) = serve(vec![ok(
        b"hello",
        "ETag: \"abc\"\r\nLast-Modified: Wed, 11 Nov 2026 12:00:00 GMT\r\n",
    )]);
    let Fetched::Body { file, validators } =
        fetch(&format!("{base}/x.zip"), &Validators::default()).unwrap()
    else {
        panic!("expected a body");
    };
    assert_eq!(std::fs::read_to_string(file.path()).unwrap(), "hello");
    assert_eq!(validators.etag.as_deref(), Some("\"abc\""));
    let head = &server.join().unwrap()[0];
    assert!(
        head.to_ascii_lowercase()
            .contains(&format!("user-agent: octa/{}", env!("CARGO_PKG_VERSION")))
    );
    assert!(!head.to_ascii_lowercase().contains("if-none-match"));
}

#[test]
fn an_unchanged_file_is_not_downloaded_again() {
    let (base, server) = serve(vec![not_modified()]);
    let known = Validators {
        etag: Some("\"abc\"".into()),
        last_modified: Some("Wed, 11 Nov 2026 12:00:00 GMT".into()),
    };
    assert!(matches!(
        fetch(&format!("{base}/x.zip"), &known).unwrap(),
        Fetched::NotModified
    ));
    let head = server.join().unwrap()[0].to_ascii_lowercase();
    assert!(head.contains("if-none-match: \"abc\""));
    assert!(head.contains("if-modified-since: wed, 11 nov 2026 12:00:00 gmt"));
}

#[test]
fn a_missing_file_is_an_error_naming_the_address() {
    let (base, _server) = serve(vec![status(404)]);
    let e = fetch(&format!("{base}/gone.zip"), &Validators::default())
        .err()
        .unwrap();
    let text = format!("{e:#}");
    assert!(text.contains("gone.zip") && text.contains("404"), "{text}");
}

#[test]
fn the_base_override_keeps_the_path() {
    assert_eq!(
        resolve_with(
            "https://data.bis.org/static/bulk/WS_XRU_csv_col.zip",
            Some("http://127.0.0.1:9/")
        ),
        "http://127.0.0.1:9/static/bulk/WS_XRU_csv_col.zip"
    );
    assert_eq!(resolve_with("https://a.b/c", None), "https://a.b/c");
}
