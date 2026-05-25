//! Tests for `octa::formats::archive_reader`. Fixtures are built
//! programmatically so no binaries land under `tests/fixtures/`.

use std::io::Write;

use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::archive_reader::{ArchiveReader, extract_entry_bytes};

fn write_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap();
}

fn write_tar(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut tar = tar::Builder::new(file);
    for (name, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, *body).unwrap();
    }
    tar.finish().unwrap();
}

fn write_tgz(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    for (name, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, *body).unwrap();
    }
    let enc = tar.into_inner().unwrap();
    enc.finish().unwrap();
}

#[test]
fn reader_claims_zip_tar_tgz_only() {
    let reader = ArchiveReader;
    assert_eq!(reader.name(), "Archive");
    let exts = reader.extensions();
    assert!(exts.contains(&"zip"));
    assert!(exts.contains(&"tar"));
    assert!(exts.contains(&"tgz"));
    // .gz alone is deliberately excluded — see archive_reader.rs.
    assert!(!exts.contains(&"gz"));
    assert!(!reader.supports_write());
}

#[test]
fn reads_zip_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zip");
    write_zip(
        &path,
        &[("a.csv", b"id,name\n1,alice\n"), ("nested/b.txt", b"hello")],
    );
    let t = ArchiveReader.read_file(&path).unwrap();
    assert_eq!(t.row_count(), 2);
    let col_names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        col_names,
        vec![
            "path",
            "size_bytes",
            "compressed_bytes",
            "mtime",
            "is_dir",
            "type",
        ]
    );

    // path + size + type columns.
    let paths: Vec<String> = (0..t.row_count())
        .map(|r| t.get(r, 0).unwrap().to_string())
        .collect();
    assert!(paths.contains(&"a.csv".to_string()));
    assert!(paths.contains(&"nested/b.txt".to_string()));

    let row_a = (0..t.row_count())
        .find(|&r| t.get(r, 0).unwrap().to_string() == "a.csv")
        .unwrap();
    assert!(matches!(t.get(row_a, 1), Some(CellValue::Int(16))));
    // compressed_bytes set for zip.
    assert!(matches!(t.get(row_a, 2), Some(CellValue::Int(_))));
    // is_dir = false for files.
    assert!(matches!(t.get(row_a, 4), Some(CellValue::Bool(false))));
    // type derived from extension.
    assert_eq!(t.get(row_a, 5).unwrap().to_string(), "csv");

    assert_eq!(t.format_name.as_deref(), Some("Archive (Zip)"));
}

#[test]
fn reads_tar_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tar");
    write_tar(
        &path,
        &[("readme.md", b"# hello"), ("data.csv", b"id\n1\n")],
    );
    let t = ArchiveReader.read_file(&path).unwrap();
    assert_eq!(t.row_count(), 2);
    // Tar has no compressed size per entry.
    let row_readme = (0..t.row_count())
        .find(|&r| t.get(r, 0).unwrap().to_string() == "readme.md")
        .unwrap();
    assert!(matches!(t.get(row_readme, 2), Some(CellValue::Null)));
    assert_eq!(t.format_name.as_deref(), Some("Archive (Tar)"));
}

#[test]
fn reads_tgz_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tgz");
    write_tgz(
        &path,
        &[("inside.json", br#"{"a": 1}"#), ("notes.txt", b"hi")],
    );
    let t = ArchiveReader.read_file(&path).unwrap();
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.format_name.as_deref(), Some("Archive (Tar+Gzip)"));
}

#[test]
fn extract_zip_entry_returns_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zip");
    write_zip(&path, &[("hello.txt", b"hello world")]);
    let bytes = extract_entry_bytes(&path, "hello.txt").unwrap();
    assert_eq!(bytes, b"hello world");
}

#[test]
fn extract_tar_entry_returns_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tar");
    write_tar(&path, &[("foo.bin", b"\x01\x02\x03\x04")]);
    let bytes = extract_entry_bytes(&path, "foo.bin").unwrap();
    assert_eq!(bytes, b"\x01\x02\x03\x04");
}

#[test]
fn extract_tgz_entry_returns_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.tgz");
    let body = b"compressed payload, but extract returns raw";
    write_tgz(&path, &[("a/b/c.txt", body)]);
    let bytes = extract_entry_bytes(&path, "a/b/c.txt").unwrap();
    assert_eq!(bytes, body);
}

#[test]
fn extract_missing_entry_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zip");
    write_zip(&path, &[("a.txt", b"x")]);
    let err = extract_entry_bytes(&path, "missing.txt").unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("not found"), "got: {}", msg);
}

#[test]
fn unsupported_extension_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blob.bin");
    std::fs::write(&path, b"x").unwrap();
    let err = ArchiveReader.read_file(&path).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.to_lowercase().contains("archive"), "got: {}", msg);
}
