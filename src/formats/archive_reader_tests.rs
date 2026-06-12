//! Unit tests for [`archive_reader`](archive_reader). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn classify_picks_extension_for_files() {
    assert_eq!(classify_path("a/b/c.csv", false), "csv");
    assert_eq!(classify_path("noext", false), "file");
    assert_eq!(classify_path("a/b/", true), "dir");
    assert_eq!(classify_path("CAPS.JSON", false), "json");
}

#[test]
fn escaping_entry_paths_are_detected() {
    assert!(entry_path_escapes("../x"));
    assert!(entry_path_escapes("a/../../x"));
    assert!(entry_path_escapes("/abs/path"));
    assert!(!entry_path_escapes("a/b.csv"));
    assert!(!entry_path_escapes("plain.txt"));
    // `a/../b` normalises inside the archive but still carries a
    // ParentDir component; refusing it is the conservative call.
    assert!(entry_path_escapes("a/../b"));
}

fn tiny_zip(dir: &Path, payload: &[u8]) -> std::path::PathBuf {
    let path = dir.join("t.zip");
    let file = File::create(&path).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("data.bin", opts).expect("start entry");
    std::io::Write::write_all(&mut zip, payload).expect("write entry");
    zip.finish().expect("finish zip");
    path
}

#[test]
fn zip_extraction_respects_the_byte_cap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = tiny_zip(dir.path(), &[7u8; 64]);
    // Under the cap: full payload comes back.
    let ok = extract_zip_entry(&path, "data.bin", 64).expect("within cap");
    assert_eq!(ok.len(), 64);
    // Over the cap: hard error, not a truncated read.
    let err = extract_zip_entry(&path, "data.bin", 63).unwrap_err();
    assert!(err.to_string().contains("extraction limit"), "{err}");
}

#[test]
fn tar_extraction_respects_the_byte_cap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("t.tar");
    let file = File::create(&path).expect("create tar");
    let mut tar = tar::Builder::new(file);
    let payload = [9u8; 64];
    let mut header = tar::Header::new_gnu();
    header.set_size(payload.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "data.bin", payload.as_slice())
        .expect("append");
    tar.finish().expect("finish tar");
    let ok = extract_tar_entry(&path, "data.bin", false, 64).expect("within cap");
    assert_eq!(ok.len(), 64);
    let err = extract_tar_entry(&path, "data.bin", false, 63).unwrap_err();
    assert!(err.to_string().contains("extraction limit"), "{err}");
}
