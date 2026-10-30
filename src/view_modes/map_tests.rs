use super::*;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// A one-shot local server answering every request with `status`.
fn serve_once(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    format!("http://{addr}/0/0/0.png")
}

#[test]
fn a_tile_server_that_answers_passes_the_probe() {
    assert_eq!(probe_tile(&serve_once("200 OK"), "octa/test"), Ok(()));
}

#[test]
fn a_refusing_or_failing_tile_server_fails_the_probe() {
    let err = probe_tile(&serve_once("404 Not Found"), "octa/test").unwrap_err();
    assert!(err.contains("404"), "{err}");
    // Port 1 on loopback: nothing listens there.
    assert!(probe_tile("http://127.0.0.1:1/0/0/0.png", "octa/test").is_err());
}

#[test]
fn the_probe_slot_reports_a_failure_once_and_then_clears() {
    let (tx, rx) = mpsc::channel();
    let mut slot = Some(rx);
    assert_eq!(take_probe_failure(&mut slot), None, "no answer yet");
    assert!(slot.is_some(), "still waiting");
    tx.send(Err("offline".to_string())).unwrap();
    assert_eq!(take_probe_failure(&mut slot), Some("offline".to_string()));
    assert!(slot.is_none(), "read once");

    let (tx, rx) = mpsc::channel();
    let mut slot = Some(rx);
    tx.send(Ok(())).unwrap();
    assert_eq!(
        take_probe_failure(&mut slot),
        None,
        "tiles load, no fallback"
    );
    assert!(slot.is_none());
}
