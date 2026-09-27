use super::*;
use crate::api::{ApiAuth, ApiPaging};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// A one-shot HTTP stub on a loopback port.
///
/// Worth the forty lines: the page loop's whole job is deciding whether there
/// is a next page, and every mode decides it from something only a real
/// response carries - a short page, a cursor in the body, a `Link` header.
/// Testing that against hand-built `Value`s would test the wrong half.
struct Stub {
    port: u16,
    handle: Option<std::thread::JoinHandle<Vec<String>>>,
}

impl Stub {
    /// Bind a port without serving yet, so a test can build a `Link` header
    /// that points back at this stub before deciding what to serve.
    fn bind() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    /// `pages` is the body (and optional Link header) to serve, in order.
    fn serve(pages: Vec<(String, Option<String>)>) -> Stub {
        let (listener, port) = Stub::bind();
        Stub::serve_on(listener, port, pages)
    }

    fn serve_on(listener: TcpListener, port: u16, pages: Vec<(String, Option<String>)>) -> Stub {
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (body, link) in pages {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                // Drain the rest of the request head.
                loop {
                    let mut l = String::new();
                    if reader.read_line(&mut l).unwrap_or(0) == 0 || l == "\r\n" {
                        break;
                    }
                    if l.to_ascii_lowercase().starts_with("authorization:") {
                        seen.push(l.trim().to_string());
                    }
                }
                seen.push(first.trim().to_string());
                let link_hdr = link.map(|l| format!("Link: {l}\r\n")).unwrap_or_default();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\n{link_hdr}Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
            seen
        });
        Stub {
            port,
            handle: Some(handle),
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn requests(mut self) -> Vec<String> {
        self.handle.take().unwrap().join().unwrap_or_default()
    }
}

fn conn(base: String, paging: ApiPaging) -> ApiConnection {
    ApiConnection {
        id: "api-t".into(),
        name: "t".into(),
        base_url: base,
        path: "rows".into(),
        paging,
        page_size: Some(2),
        timeout_secs: 5,
        ..Default::default()
    }
}

fn run(c: &ApiConnection) -> FetchOutcome {
    let cancel = Arc::new(AtomicBool::new(false));
    fetch_table(c, None, &FetchOptions::default(), &cancel).expect("fetch")
}

fn page(ids: &[i64]) -> String {
    let rows: Vec<_> = ids.iter().map(|i| json!({"id": i})).collect();
    Value::Array(rows).to_string()
}

#[test]
fn one_page_when_there_is_no_pagination() {
    let s = Stub::serve(vec![(page(&[1, 2]), None)]);
    let out = run(&conn(s.base(), ApiPaging::None));
    assert_eq!(out.table.row_count(), 2);
    assert_eq!(out.pages, 1);
    assert!(!out.capped);
}

#[test]
fn page_numbers_walk_until_a_page_is_empty() {
    let s = Stub::serve(vec![
        (page(&[1, 2]), None),
        (page(&[3, 4]), None),
        (page(&[]), None),
    ]);
    let c = conn(
        s.base(),
        ApiPaging::PageNumber {
            param: "page".into(),
            start: 1,
        },
    );
    let out = run(&c);
    assert_eq!(out.table.row_count(), 4);
    assert_eq!(out.pages, 3, "the empty page is what ends it");
    let reqs = s.requests();
    assert!(reqs.iter().any(|r| r.contains("page=1")), "{reqs:?}");
    assert!(reqs.iter().any(|r| r.contains("page=3")), "{reqs:?}");
}

#[test]
fn offset_limit_stops_on_a_short_page() {
    let s = Stub::serve(vec![(page(&[1, 2]), None), (page(&[3]), None)]);
    let c = conn(
        s.base(),
        ApiPaging::OffsetLimit {
            offset_param: "offset".into(),
            limit_param: "limit".into(),
        },
    );
    let out = run(&c);
    assert_eq!(out.table.row_count(), 3);
    assert_eq!(out.pages, 2, "a short page is the last page");
    let reqs = s.requests();
    assert!(reqs.iter().any(|r| r.contains("offset=0")), "{reqs:?}");
    assert!(reqs.iter().any(|r| r.contains("offset=2")), "{reqs:?}");
}

#[test]
fn a_cursor_in_the_body_leads_to_the_next_page() {
    let first = json!({"rows": [{"id": 1}], "meta": {"next": "abc"}}).to_string();
    let second = json!({"rows": [{"id": 2}], "meta": {}}).to_string();
    let s = Stub::serve(vec![(first, None), (second, None)]);
    let mut c = conn(
        s.base(),
        ApiPaging::Cursor {
            pointer: "/meta/next".into(),
            param: "cursor".into(),
        },
    );
    c.records_pointer = "/rows".into();
    let out = run(&c);
    assert_eq!(out.table.row_count(), 2);
    assert_eq!(out.pages, 2);
    let reqs = s.requests();
    assert!(reqs.iter().any(|r| r.contains("cursor=abc")), "{reqs:?}");
}

#[test]
fn a_link_header_leads_to_the_next_page() {
    let (listener, port) = Stub::bind();
    let base = format!("http://127.0.0.1:{port}");
    let link = format!("<{base}/rows?page=2>; rel=\"next\"");
    let s = Stub::serve_on(
        listener,
        port,
        vec![(page(&[1]), Some(link)), (page(&[2]), None)],
    );
    let out = run(&conn(s.base(), ApiPaging::LinkHeader));
    assert_eq!(out.table.row_count(), 2);
    assert_eq!(out.pages, 2, "the header, then its absence, drove the loop");
    let reqs = s.requests();
    assert!(reqs.iter().any(|r| r.contains("page=2")), "{reqs:?}");
}

/// A next-link is data the server chose, so it must not be able to walk the
/// fetch loop onto a host the user never saved.
#[test]
fn a_next_link_that_leaves_the_origin_is_refused() {
    let s = Stub::serve(vec![(
        page(&[1]),
        Some("<https://evil.example.com/rows>; rel=\"next\"".to_string()),
    )]);
    let c = conn(s.base(), ApiPaging::LinkHeader);
    let cancel = Arc::new(AtomicBool::new(false));
    let err = fetch_table(&c, None, &FetchOptions::default(), &cancel)
        .err()
        .expect("refuses to leave the origin")
        .to_string();
    assert!(err.contains("evil.example.com"), "{err}");
    assert!(err.contains("not the host"), "{err}");
}

#[test]
fn the_row_cap_stops_the_loop_and_says_so() {
    let s = Stub::serve(vec![(page(&[1, 2, 3, 4, 5]), None)]);
    let c = conn(s.base(), ApiPaging::None);
    let cancel = Arc::new(AtomicBool::new(false));
    let out = fetch_table(
        &c,
        None,
        &FetchOptions {
            max_rows: Some(3),
            ..Default::default()
        },
        &cancel,
    )
    .expect("fetch");
    assert_eq!(out.table.row_count(), 3);
    assert!(out.capped, "a truncated read must not look complete");
}

#[test]
fn the_page_cap_stops_a_server_that_never_ends() {
    let s = Stub::serve(vec![(page(&[1]), None), (page(&[2]), None)]);
    let c = conn(
        s.base(),
        ApiPaging::PageNumber {
            param: "page".into(),
            start: 1,
        },
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let out = fetch_table(
        &c,
        None,
        &FetchOptions {
            max_pages: Some(2),
            ..Default::default()
        },
        &cancel,
    )
    .expect("fetch");
    assert_eq!(out.pages, 2);
    assert!(out.capped);
}

#[test]
fn a_http_error_names_the_status_and_the_message() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let body = json!({"message": "bad token"}).to_string();
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    let c = conn(format!("http://127.0.0.1:{port}"), ApiPaging::None);
    let cancel = Arc::new(AtomicBool::new(false));
    let err = fetch_table(&c, None, &FetchOptions::default(), &cancel)
        .err()
        .expect("a 401 is an error")
        .to_string();
    assert!(err.contains("401"), "{err}");
    assert!(
        err.contains("bad token"),
        "the server's reason survives: {err}"
    );
}

#[test]
fn bearer_auth_reaches_the_wire() {
    let s = Stub::serve(vec![(page(&[1]), None)]);
    let mut c = conn(s.base(), ApiPaging::None);
    c.auth = ApiAuth::Bearer;
    let cancel = Arc::new(AtomicBool::new(false));
    fetch_table(&c, Some("tok"), &FetchOptions::default(), &cancel).expect("fetch");
    let reqs = s.requests();
    assert!(
        reqs.iter()
            .any(|r| r.eq_ignore_ascii_case("Authorization: Bearer tok")),
        "{reqs:?}"
    );
}

#[test]
fn each_auth_mode_puts_the_secret_where_it_belongs() {
    let base = ApiConnection::default();
    let hdr = |a: ApiAuth| {
        let c = ApiConnection {
            auth: a,
            ..base.clone()
        };
        auth_parts(&c, Some("s"))
    };
    assert_eq!(hdr(ApiAuth::None), (vec![], vec![]));
    assert_eq!(
        hdr(ApiAuth::Bearer).0,
        vec![("Authorization".to_string(), "Bearer s".to_string())]
    );
    assert_eq!(
        hdr(ApiAuth::HeaderKey {
            name: "X-Key".into()
        })
        .0,
        vec![("X-Key".to_string(), "s".to_string())]
    );
    assert_eq!(
        hdr(ApiAuth::QueryKey { name: "k".into() }).1,
        vec![("k".to_string(), "s".to_string())]
    );
    // base64("u:s")
    assert_eq!(
        hdr(ApiAuth::Basic {
            username: "u".into()
        })
        .0,
        vec![("Authorization".to_string(), "Basic dTpz".to_string())]
    );
}

#[test]
fn query_parameters_are_escaped_and_appended() {
    assert_eq!(with_query("http://h/p", &[]), "http://h/p");
    assert_eq!(
        with_query("http://h/p", &[("a".into(), "b c".into())]),
        "http://h/p?a=b%20c"
    );
    assert_eq!(
        with_query("http://h/p?x=1", &[("a".into(), "&b".into())]),
        "http://h/p?x=1&a=%26b",
        "an existing query is kept and the value cannot inject a parameter"
    );
}

#[test]
fn link_header_parsing_finds_only_next() {
    let h = "<http://h/p?page=1>; rel=\"prev\", <http://h/p?page=3>; rel=\"next\"";
    assert_eq!(next_from_link(h).as_deref(), Some("http://h/p?page=3"));
    assert_eq!(next_from_link("<http://h/p>; rel=\"last\""), None);
    assert_eq!(next_from_link("nonsense"), None);
}
