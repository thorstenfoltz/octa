//! Live-server tests for the SFTP and WebDAV storage types. Each runs only
//! when its env var is set, and otherwise prints "skipped" and passes, like
//! `db_live_tests.rs`:
//!
//! ```bash
//! docker run -d -p 2222:22 -e SFTP_USERS='octa:octa_test_pw:::upload' atmoz/sftp
//! docker run -d -p 8080:80 -e AUTH_TYPE=Basic -e USERNAME=octa \
//!     -e PASSWORD=octa_test_pw bytemark/webdav
//! export OCTA_TEST_SFTP_URL='host=127.0.0.1;port=2222;user=octa;pass=octa_test_pw;dir=upload'
//! export OCTA_TEST_WEBDAV_URL='url=http://127.0.0.1:8080/;user=octa;pass=octa_test_pw'
//! ```
//!
//! The SFTP test accepts the container's host key on first sight, which adds
//! one line for 127.0.0.1:2222 to your ~/.ssh/known_hosts. Both leave an empty
//! `octa-live-<pid>` folder behind: deleting a folder's files is what the
//! sidebar does, folders themselves are not deleted.

use std::collections::HashMap;

use std::sync::Arc;

use object_store::memory::InMemory;
use octa::cloud::{
    CloudConnection, CloudKind, CloudProvider, ObjectStoreProvider, ProviderCreds, build_provider,
    ops, upload_file,
};

fn spec(var: &str) -> Option<HashMap<String, String>> {
    let raw = std::env::var(var).ok()?;
    Some(
        raw.split(';')
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect(),
    )
}

/// Every operation the sidebar uses, in a scratch folder under `root`.
fn exercise(p: &dyn CloudProvider, root: &str) {
    let dir = format!("{root}octa-live-{}/", std::process::id());
    let a = format!("{dir}a.csv");
    p.put(&a, b"id\n1\n".to_vec()).expect("put");
    assert_eq!(p.get(&a).expect("get"), b"id\n1\n");

    // Save back over an existing file.
    p.put(&a, b"id\n2\n".to_vec()).expect("overwrite");
    assert_eq!(p.get(&a).unwrap(), b"id\n2\n");

    // Streaming upload into a folder that does not exist yet.
    let b = format!("{dir}sub/b.csv");
    let mut up = p.put_streaming(&b).expect("put_streaming");
    up.write_chunk(b"x\n".to_vec()).unwrap();
    up.write_chunk(b"y\n".to_vec()).unwrap();
    up.finish().expect("finish");
    assert_eq!(p.get(&b).unwrap(), b"x\ny\n");

    let names: Vec<String> = p
        .list(&dir)
        .expect("list")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&"a.csv".to_string()), "{names:?}");
    assert!(names.contains(&"sub".to_string()), "{names:?}");
    let (all, truncated) = p.list_recursive(&dir, 100).expect("list_recursive");
    assert!(!truncated);
    assert_eq!(all.len(), 2, "{all:?}");

    // Copy and move inside the store.
    let c = format!("{dir}c.csv");
    ops::copy(p, &a, p, &c, true).expect("copy");
    let d = format!("{dir}moved/d.csv");
    ops::move_(p, &c, p, &d, true).expect("move");
    assert_eq!(p.get(&d).unwrap(), b"id\n2\n");
    assert!(p.get(&c).is_err(), "the moved source is gone");

    // Delete everything, then delete a missing file, which is not an error.
    ops::delete(p, &dir).expect("delete the folder's files");
    p.delete(&a).expect("deleting a missing file is fine");
}

/// A bucket to the server and back, the way Copy to... runs between a bucket
/// and a file server. The bucket is in memory: the same provider type S3,
/// Azure and GCS go through. Then a local file uploaded in blocks, as File >
/// Upload to cloud and `--cloud-put` do.
fn across(server: &dyn CloudProvider, root: &str) {
    let bucket = ObjectStoreProvider::new(Arc::new(InMemory::new()));
    bucket.put("in/t.csv", b"id\n1\n".to_vec()).unwrap();
    let dir = format!("{root}octa-across-{}/", std::process::id());
    ops::copy(&bucket, "in/t.csv", server, &format!("{dir}t.csv"), false)
        .expect("bucket to server");
    ops::copy(server, &dir, &bucket, "back/", false).expect("server folder to bucket");
    assert_eq!(bucket.get("back/t.csv").unwrap(), b"id\n1\n");
    ops::move_(
        &bucket,
        "back/t.csv",
        server,
        &format!("{dir}moved.csv"),
        false,
    )
    .expect("move bucket to server");
    assert!(
        bucket.get("back/t.csv").is_err(),
        "the moved source is gone"
    );

    let local = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(local.path(), b"x\n2\n").unwrap();
    let up = format!("{dir}up/u.csv");
    assert_eq!(
        upload_file(server, local.path(), &up).expect("upload_file"),
        4
    );
    assert_eq!(server.get(&up).unwrap(), b"x\n2\n");
    ops::delete(server, &dir).expect("clean up");
}

#[test]
fn sftp_round_trip() {
    let Some(s) = spec("OCTA_TEST_SFTP_URL") else {
        println!("skipped: OCTA_TEST_SFTP_URL not set");
        return;
    };
    let port: u16 = s.get("port").and_then(|p| p.parse().ok()).unwrap_or(22);
    let conn = CloudConnection {
        id: "live-sftp".into(),
        name: "live sftp".into(),
        kind: CloudKind::Sftp,
        bucket: octa::cloud::sftp_authority(&s["host"], port),
        username: Some(s["user"].clone()),
        ssh_auth: Some(octa::ssh::SshAuth::Password),
        accept_new_host_key: true,
        allow_writes: true,
        ..Default::default()
    };
    let p = build_provider(&conn, &ProviderCreds::Password(s["pass"].clone())).expect("connect");
    let root = s
        .get("dir")
        .map(|d| format!("{}/", d.trim_matches('/')))
        .unwrap_or_default();
    exercise(p.as_ref(), &root);
    across(p.as_ref(), &root);
}

#[test]
fn webdav_round_trip() {
    let Some(s) = spec("OCTA_TEST_WEBDAV_URL") else {
        println!("skipped: OCTA_TEST_WEBDAV_URL not set");
        return;
    };
    let url = s["url"].clone();
    let (_, authority, prefix) = octa::cloud::split_server_url(&url, true).unwrap();
    let conn = CloudConnection {
        id: "live-dav".into(),
        name: "live dav".into(),
        kind: CloudKind::WebDav,
        bucket: authority,
        prefix: (!prefix.is_empty()).then(|| prefix.clone()),
        endpoint: Some(url),
        allow_http: true,
        username: Some(s["user"].clone()),
        allow_writes: true,
        ..Default::default()
    };
    let p = build_provider(&conn, &ProviderCreds::Password(s["pass"].clone())).expect("client");
    exercise(p.as_ref(), &prefix);
    across(p.as_ref(), &prefix);
}
