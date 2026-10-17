use super::backup_existing_file;

#[test]
fn copies_when_present_noops_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("data.csv");
    // Missing file: no backup, Ok(None).
    assert!(backup_existing_file(&f).unwrap().is_none());

    std::fs::write(&f, b"a,b\n1,2\n").unwrap();
    let bak1 = backup_existing_file(&f).unwrap().expect("backup made");
    assert!(bak1.exists());
    assert_eq!(std::fs::read(&bak1).unwrap(), b"a,b\n1,2\n");
    assert!(
        bak1.file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".bak-"),
        "sidecar name carries .bak-: {bak1:?}"
    );
    // A second backup in the same second must not clobber the first.
    let bak2 = backup_existing_file(&f).unwrap().expect("second backup");
    assert_ne!(bak1, bak2, "backup names are unique");
    assert!(bak2.exists());
}
