use super::*;
use std::io::{BufRead, BufReader, Read, Write};

#[test]
fn a_nextcloud_address_splits_into_root_host_and_folder() {
    let (root, host, prefix) = split_server_url(
        "https://cloud.example.com/remote.php/dav/files/anna/",
        false,
    )
    .unwrap();
    assert_eq!(root, "https://cloud.example.com/");
    assert_eq!(host, "cloud.example.com");
    assert_eq!(prefix, "remote.php/dav/files/anna/");
    let (_, host, prefix) = split_server_url("davs://nas:5006", false).unwrap();
    assert_eq!((host.as_str(), prefix.as_str()), ("nas:5006", ""));
}

#[test]
fn plain_http_needs_the_allow_http_tick() {
    assert!(split_server_url("http://nas/dav/", false).is_err());
    let (root, _, _) = split_server_url("http://nas/dav/", true).unwrap();
    assert_eq!(root, "http://nas/");
    assert!(split_server_url("ftp://nas/", true).is_err());
    assert!(split_server_url("https:///x", false).is_err());
}

#[test]
fn basic_auth_is_the_rfc_7617_form() {
    assert_eq!(
        basic_auth("Aladdin", "open sesame"),
        "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
    );
}

#[test]
fn upload_urls_encode_each_segment() {
    assert_eq!(
        url_for("https://h/", "a b/c+d.csv"),
        "https://h/a%20b/c%2Bd.csv"
    );
    assert_eq!(url_for("https://h/", "x-y_z.~1"), "https://h/x-y_z.~1");
}

/// The streamed upload sends one PUT with the right path, credentials
/// and the blocks joined, straight from the spool file.
#[test]
fn a_streamed_upload_is_one_authenticated_put() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let seen = std::thread::spawn(move || {
        let (stream, _) = server.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = Vec::new();
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
            head.push(line);
        }
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).unwrap();
        let mut out = stream;
        out.write_all(b"HTTP/1.1 201 Created\r\ncontent-length: 0\r\n\r\n")
            .unwrap();
        (head, body)
    });

    let url = format!("http://127.0.0.1:{port}/dav/");
    let p = build_webdav_provider(&url, true, Some("anna"), Some("pw")).unwrap();
    let mut up = p.put_streaming("dav/folder/x.csv").unwrap();
    up.write_chunk(b"a,b\n".to_vec()).unwrap();
    up.write_chunk(b"1,2\n".to_vec()).unwrap();
    up.finish().unwrap();

    let (head, body) = seen.join().unwrap();
    assert_eq!(head[0].trim_end(), "PUT /dav/folder/x.csv HTTP/1.1");
    let auth = basic_auth("anna", "pw").to_ascii_lowercase();
    assert!(
        head.iter()
            .any(|h| h.to_ascii_lowercase().trim_end() == format!("authorization: {auth}")),
        "{head:?}"
    );
    assert_eq!(body, b"a,b\n1,2\n");
}
