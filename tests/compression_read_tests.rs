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

#[test]
fn a_failed_write_leaves_the_original_file_intact() {
    // The whole point of the temp + rename: `File::create` used to truncate
    // the user's file before the first byte was written.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("precious.csv");
    std::fs::write(&target, "id,name\n1,alice\n").unwrap();

    let err = octa::formats::write_atomically(&target, |tmp| -> anyhow::Result<()> {
        std::fs::write(tmp, "half a fi").unwrap();
        anyhow::bail!("disk full")
    })
    .unwrap_err();
    assert!(err.to_string().contains("disk full"));

    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "id,name\n1,alice\n",
        "the original must survive a failed write"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .filter(|n| n != "precious.csv")
        .collect();
    assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
}

#[test]
fn a_successful_write_replaces_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("data.csv");
    std::fs::write(&target, "old\n").unwrap();
    octa::formats::write_atomically(&target, |tmp| {
        std::fs::write(tmp, "new\n")?;
        Ok::<(), anyhow::Error>(())
    })
    .unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new\n");
}
