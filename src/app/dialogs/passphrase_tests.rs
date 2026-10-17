use super::*;
use crate::app::state::{ProtectedOrigin, TabState};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn origin(name: &str) -> ProtectedOrigin {
    let temp = Arc::new(octa::formats::encrypt::ProtectedTempDir::new().unwrap());
    ProtectedOrigin {
        zip: PathBuf::from(format!("/data/{name}.zip")),
        entry: name.into(),
        passphrase: "pw".into(),
        temp_file: temp.path().join(name),
        _temp: temp,
    }
}

/// Two protected opens can wait at once; each table claims only its own.
#[test]
fn a_tab_claims_only_the_origin_of_its_own_plain_copy() {
    let a = origin("a.csv");
    let b = origin("b.csv");
    let (a_path, b_path) = (a.temp_file.clone(), b.temp_file.clone());
    let mut pending = vec![a, b];

    // Some other file: nothing claimed, both still wait.
    assert!(take_protected_origin(&mut pending, &[], Path::new("/data/other.csv")).is_none());
    assert_eq!(pending.len(), 2);

    // The second open's table arrives first: the first one keeps waiting.
    let got = take_protected_origin(&mut pending, &[], &b_path).expect("b's origin");
    assert_eq!(got.zip, PathBuf::from("/data/b.csv.zip"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].temp_file, a_path);
}

/// A re-read of a plain copy (Refresh, Reopen as, another sheet) shares the
/// tab's origin, so the folder outlives the old tab and goes with the last.
#[test]
fn a_reread_shares_the_origin_and_its_folder() {
    let o = origin("r.csv");
    let path = o.temp_file.clone();
    let dir = path.parent().unwrap().to_path_buf();
    let mut tab = TabState::new(Default::default());
    tab.protected_origin = Some(o);
    let mut tabs = vec![tab];

    let shared = take_protected_origin(&mut Vec::new(), &tabs, &path).expect("the tab's origin");
    assert_eq!(shared.zip, PathBuf::from("/data/r.csv.zip"));

    tabs.clear();
    assert!(dir.exists(), "the re-read tab still holds the folder");
    drop(shared);
    assert!(!dir.exists(), "the last holder removes it");
}

/// Large-file mode asks this before converting: a decrypted copy, pending or
/// open, is never converted into a plain Parquet in the shared temp folder.
#[test]
fn a_protected_copy_is_known_pending_or_open() {
    let pending = vec![origin("p.csv")];
    let mut tab = TabState::new(Default::default());
    tab.protected_origin = Some(origin("t.avro"));
    let tabs = vec![tab];
    let open_copy = tabs[0].protected_origin.as_ref().unwrap().temp_file.clone();

    assert!(is_protected_copy(&pending, &tabs, &pending[0].temp_file));
    assert!(is_protected_copy(&pending, &tabs, &open_copy));
    assert!(!is_protected_copy(
        &pending,
        &tabs,
        Path::new("/data/big.avro")
    ));
}

/// A protected open whose read failed or whose picker was cancelled lets its
/// plain copy go; one still being opened keeps it.
#[test]
fn an_open_nothing_is_running_for_lets_its_copy_go() {
    let waiting = origin("w.csv");
    let failed = origin("f.csv");
    let waiting_path = waiting.temp_file.clone();
    let failed_dir = failed.temp_file.parent().unwrap().to_path_buf();
    let mut pending = vec![waiting, failed];

    retain_in_flight(&mut pending, &[None, Some(waiting_path.as_path()), None]);

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].temp_file, waiting_path);
    assert!(!failed_dir.exists(), "the failed open's folder is gone");
}

/// A compressed or archive entry is never the one opened: opening it would
/// put plain data into the shared temp folder. A readable entry beside it is
/// taken instead; with none, the open says there is nothing to read.
#[test]
fn a_compressed_entry_inside_an_encrypted_zip_is_never_opened() {
    let dir = tempfile::tempdir().unwrap();
    let blob = dir.path().join("blob");
    let csv = dir.path().join("notes.csv");
    std::fs::write(&blob, b"x").unwrap();
    std::fs::write(&csv, b"a,b\n1,2\n").unwrap();
    let secret = octa::formats::decrypt::OpenSecret::Passphrase("pw".into());
    let unopenable = [
        "data.csv.gz",
        "data.jsonl.zst",
        "inner.zip",
        "inner.TAR",
        "inner.tgz",
        "inner.tar.gz",
    ];

    let only_unopenable = dir.path().join("unopenable.zip");
    let entries: Vec<(&str, &Path)> = unopenable.iter().map(|n| (*n, blob.as_path())).collect();
    octa::formats::encrypt::write_encrypted_zip(&only_unopenable, &entries, "pw").unwrap();
    assert_eq!(
        decrypt_first_zip_entry(&only_unopenable, &secret).unwrap_err(),
        t("passphrase.no_readable_entry")
    );

    let mixed = dir.path().join("mixed.zip");
    octa::formats::encrypt::write_encrypted_zip(
        &mixed,
        &[
            ("inner.zip", &blob),
            ("data.csv.gz", &blob),
            ("notes.csv", &csv),
        ],
        "pw",
    )
    .unwrap();
    let (entry, _) = decrypt_first_zip_entry(&mixed, &secret).unwrap();
    assert_eq!(entry, "notes.csv");
}
