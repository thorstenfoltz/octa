//! Transparent decompression through the shared read path: a `.csv.gz` /
//! `.jsonl.zst` opens via `read_table_auto` exactly like its inner format.

use std::io::Write;

use octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES;
use octa::formats::read_table_auto;

fn write_gz(path: &std::path::Path, payload: &[u8]) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    enc.write_all(payload).unwrap();
    enc.finish().unwrap();
}

fn write_zst(path: &std::path::Path, payload: &[u8]) {
    let f = std::fs::File::create(path).unwrap();
    let mut enc = zstd::stream::write::Encoder::new(f, 3).unwrap();
    enc.write_all(payload).unwrap();
    enc.finish().unwrap();
}

#[test]
fn gzipped_csv_reads_like_csv() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv.gz");
    write_gz(&path, b"name,age\nada,36\ngrace,45\n");
    let t = read_table_auto(&path, None, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(t.row_count(), 2);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["name", "age"]);
    // Provenance points at the file the user named, not the temp.
    assert_eq!(
        t.source_path.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
}

#[test]
fn zstd_jsonl_reads_like_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.jsonl.zst");
    write_zst(&path, b"{\"x\": 1}\n{\"x\": 2}\n{\"x\": 3}\n");
    let t = read_table_auto(&path, None, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(t.row_count(), 3);
    assert!(t.columns.iter().any(|c| c.name == "x"));
}

#[test]
fn plain_csv_passthrough_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.csv");
    std::fs::write(&path, "a\n1\n").unwrap();
    let t = read_table_auto(&path, None, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(t.row_count(), 1);
}

#[test]
fn cap_hit_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.csv.gz");
    let payload = format!("a\n{}", "1\n".repeat(5000));
    write_gz(&path, payload.as_bytes());
    let err = read_table_auto(&path, None, 100).unwrap_err().to_string();
    assert!(err.contains("decompressing"), "{err}");
}
