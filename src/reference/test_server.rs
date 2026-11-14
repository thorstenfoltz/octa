//! A stand-in HTTP server for the reference tests. Never the internet.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// A one-shot stand-in server: answers each connection with the next canned
/// response and hands back the request heads it saw.
pub(crate) fn serve(responses: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut head = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                head.push_str(&line);
            }
            seen.push(head);
            stream.write_all(&response).unwrap();
        }
        seen
    });
    (base, handle)
}

pub(crate) fn ok(body: &[u8], extra: &str) -> Vec<u8> {
    let mut r = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    )
    .into_bytes();
    r.extend_from_slice(body);
    r
}

pub(crate) fn not_modified() -> Vec<u8> {
    b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".to_vec()
}

pub(crate) fn status(code: u16) -> Vec<u8> {
    format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
}

/// One file in a zip, as GeoNames and BIS ship them.
pub(crate) fn zip_of(name: &str, body: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut w = zip::ZipWriter::new(&mut buf);
    w.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    w.write_all(body.as_bytes()).unwrap();
    w.finish().unwrap();
    buf.into_inner()
}
