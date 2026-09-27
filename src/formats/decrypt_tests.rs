use super::*;

/// Build an AES-encrypted zip holding one small CSV, so the tests do not
/// depend on a checked-in binary fixture.
fn encrypted_zip(dir: &Path, passphrase: &str) -> std::path::PathBuf {
    use std::io::Write;
    let path = dir.join("encrypted.zip");
    let file = std::fs::File::create(&path).expect("create zip");
    let mut w = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, passphrase);
    w.start_file("data.csv", opts).expect("start entry");
    w.write_all(b"id,name\n1,alice\n").expect("write entry");
    w.finish().expect("finish zip");
    path
}

fn plain_zip(dir: &Path) -> std::path::PathBuf {
    use std::io::Write;
    let path = dir.join("plain.zip");
    let file = std::fs::File::create(&path).expect("create zip");
    let mut w = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    w.start_file("data.csv", opts).expect("start entry");
    w.write_all(b"id,name\n1,alice\n").expect("write entry");
    w.finish().expect("finish zip");
    path
}

#[test]
fn an_encrypted_zip_is_detected_before_any_prompt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = encrypted_zip(dir.path(), "hunter2");
    assert!(
        zip_needs_passphrase(&p).expect("readable"),
        "detection must not need the passphrase"
    );
}

/// Prompting for a file that needs nothing is its own small insult, so the
/// negative case matters as much as the positive one.
#[test]
fn a_plain_zip_needs_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = plain_zip(dir.path());
    assert!(!zip_needs_passphrase(&p).expect("readable"));
}

#[test]
fn the_right_passphrase_yields_the_entry_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = encrypted_zip(dir.path(), "hunter2");
    let bytes = open_zip_entry(&p, "data.csv", &OpenSecret::Passphrase("hunter2".into()))
        .expect("decrypts");
    assert!(String::from_utf8_lossy(&bytes).starts_with("id,"));
}

#[test]
fn a_wrong_passphrase_is_an_error_not_garbage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = encrypted_zip(dir.path(), "hunter2");
    let err = open_zip_entry(&p, "data.csv", &OpenSecret::Passphrase("wrong".into()))
        .expect_err("must not succeed");
    let msg = format!("{err:#}");
    assert!(
        !msg.contains("wrong"),
        "the error must never echo the passphrase: {msg}"
    );
}

/// The same rule for the right passphrase: an error on a later step must not
/// carry it either.
#[test]
fn no_error_path_echoes_the_passphrase() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = encrypted_zip(dir.path(), "hunter2");
    let err = open_zip_entry(&p, "missing.csv", &OpenSecret::Passphrase("hunter2".into()))
        .expect_err("no such entry");
    let msg = format!("{err:#}");
    assert!(!msg.contains("hunter2"), "leaked the passphrase: {msg}");
}

/// A plain zip opens with no secret at all, which is the ordinary path.
#[test]
fn a_plain_zip_opens_without_a_secret() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = plain_zip(dir.path());
    let bytes = open_zip_entry(&p, "data.csv", &OpenSecret::None).expect("reads");
    assert!(String::from_utf8_lossy(&bytes).starts_with("id,"));
}

/// A plain xlsx is a zip, not an OLE container, so it must not be mistaken
/// for a protected one and trigger a pointless prompt.
#[test]
fn a_plain_xlsx_is_not_mistaken_for_a_protected_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = plain_zip(dir.path());
    assert!(!xlsx_needs_passphrase(&p).expect("readable"));
}

/// An OLE compound file is what a protected workbook actually is. Detection
/// still runs even though Octa cannot open one, so the file can be named
/// for what it is instead of reported as corrupt.
#[test]
fn an_ole_container_is_detected_as_protected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("protected.xlsx");
    let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    bytes.extend(std::iter::repeat_n(0u8, 512));
    std::fs::write(&p, &bytes).expect("written");
    assert!(xlsx_needs_passphrase(&p).expect("readable"));
}

/// A file too short to be either is not protected, and is not an error.
#[test]
fn a_tiny_file_is_not_protected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("tiny.xlsx");
    std::fs::write(&p, b"PK").expect("written");
    assert!(!xlsx_needs_passphrase(&p).expect("readable"));
}

/// A debug report is a file people paste into an issue tracker, so the
/// redactor is the second lock on the same door: the decryption paths are
/// written never to put a passphrase in an error, and this catches anything
/// that reaches a log by another route.
#[test]
fn a_passphrase_never_reaches_the_redacted_report() {
    let line = "opening /tmp/x.xlsx with passphrase hunter2";
    let out = crate::diagnostics::report::redact(line, None, None);
    assert!(
        !out.contains("hunter2"),
        "passphrase leaked into diagnostics: {out}"
    );
}

#[test]
fn the_redactor_catches_the_labelled_forms_too() {
    for line in [
        "password: hunter2",
        "passphrase=hunter2",
        "PASSWORD hunter2",
    ] {
        let out = crate::diagnostics::report::redact(line, None, None);
        assert!(!out.contains("hunter2"), "{line} -> {out}");
    }
}
