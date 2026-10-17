use super::file_stamp;

/// The stamp has to be stable for an untouched file and move for a
/// rewritten one, or the Save guard either nags on every save or never
/// fires at all.
#[test]
fn a_stamp_is_stable_until_the_file_is_rewritten() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"one,two\n1,2\n").unwrap();
    let first = file_stamp(f.path()).expect("stamped");
    assert_eq!(file_stamp(f.path()), Some(first), "nothing touched it");

    std::fs::write(f.path(), b"one,two\n1,2\n3,4\n").unwrap();
    assert_ne!(file_stamp(f.path()), Some(first), "rewritten behind us");
}

/// A file that is gone stamps as `None`, which the Save guard reads as
/// "nothing to lose, recreate it" rather than as a conflict.
#[test]
fn a_missing_file_has_no_stamp() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(file_stamp(&dir.path().join("not-here.csv")), None);
}
