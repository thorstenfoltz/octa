use super::*;

/// Write `file` with `body` and a distinct modification time, so each
/// kept version gets its own stamp.
fn write(file: &Path, body: &str, minute: u32) {
    std::fs::write(file, body).unwrap();
    let t = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_secs(1_800_000_000 + u64::from(minute) * 60);
    std::fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

#[test]
fn keeps_the_replaced_content_newest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let file = tmp.path().join("orders.csv");
    let none = Limits::default();
    assert_eq!(
        keep(&root, &file, none).unwrap(),
        None,
        "nothing to keep yet"
    );

    write(&file, "a\n1\n", 0);
    keep(&root, &file, none).unwrap();
    write(&file, "a\n2\n", 1);
    keep(&root, &file, none).unwrap();

    let versions = list(&root, &file);
    assert_eq!(versions.len(), 2);
    assert_eq!(
        std::fs::read_to_string(&versions[0].path).unwrap(),
        "a\n2\n"
    );
    assert_eq!(
        std::fs::read_to_string(&versions[1].path).unwrap(),
        "a\n1\n"
    );
    assert!(versions[0].path.to_string_lossy().ends_with("-orders.csv"));
    assert!(versions[0].when > versions[1].when);
    // Another file has its own list.
    assert!(list(&root, &tmp.path().join("other.csv")).is_empty());
}

#[test]
fn the_per_file_limit_drops_the_oldest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let file = tmp.path().join("t.csv");
    let limits = Limits {
        per_file: Some(2),
        total_bytes: None,
    };
    for i in 0..4 {
        write(&file, &format!("v{i}"), i);
        keep(&root, &file, limits).unwrap();
    }
    let bodies: Vec<String> = list(&root, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["v3", "v2"]);
}

#[test]
fn the_size_limit_drops_the_oldest_but_keeps_each_files_newest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let a = tmp.path().join("a.csv");
    let b = tmp.path().join("b.csv");
    let none = Limits::default();
    write(&a, "aaaaaaaaaa", 0); // 10 bytes each
    keep(&root, &a, none).unwrap();
    write(&b, "bbbbbbbbbb", 1);
    keep(&root, &b, none).unwrap();
    write(&a, "AAAAAAAAAA", 2);
    keep(&root, &a, none).unwrap();

    // 30 bytes kept, 15 allowed: the oldest (a's first) goes, then the
    // rest are each file's newest and stay although still over.
    prune(
        &root,
        Limits {
            per_file: None,
            total_bytes: Some(15),
        },
    )
    .unwrap();
    let a_left: Vec<String> = list(&root, &a)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(a_left, ["AAAAAAAAAA"]);
    assert_eq!(list(&root, &b).len(), 1);
}

#[test]
fn restore_puts_the_version_back_and_keeps_the_current_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let file = tmp.path().join("t.csv");
    let none = Limits::default();
    write(&file, "old", 0);
    keep(&root, &file, none).unwrap();
    write(&file, "new", 1);

    let old = list(&root, &file)[0].path.clone();
    restore(&root, &old, &file, none).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    let bodies: Vec<String> = list(&root, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(
        bodies,
        ["new", "old"],
        "the replaced 'new' is a version now"
    );
}

#[test]
fn restoring_the_oldest_version_at_the_limit_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let file = tmp.path().join("t.csv");
    let one = Limits {
        per_file: Some(1),
        total_bytes: None,
    };
    write(&file, "old", 0);
    keep(&root, &file, one).unwrap();
    write(&file, "new", 1);

    let old = list(&root, &file)[0].path.clone();
    restore(&root, &old, &file, one).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    let bodies: Vec<String> = list(&root, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["new"], "the limit holds once the version is back");
}

#[test]
fn a_folder_with_no_versions_left_is_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let file = tmp.path().join("t.csv");
    write(&file, "x", 0);
    keep(&root, &file, Limits::default()).unwrap();
    let dir = dir_for(&root, &file);
    std::fs::remove_file(&list(&root, &file)[0].path).unwrap();
    prune(&root, Limits::default()).unwrap();
    assert!(!dir.exists());
}

#[test]
fn stamps_parse_and_other_files_are_ignored() {
    assert_eq!(
        parse_stamp("20261007-140211-123-orders.csv").as_deref(),
        Some("2026-10-07 14:02:11")
    );
    assert_eq!(parse_stamp("source.txt"), None);
}
