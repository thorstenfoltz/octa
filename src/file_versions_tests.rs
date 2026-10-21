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
    let store = Store::Shared(root.clone());
    let file = tmp.path().join("orders.csv");
    let none = Limits::default();
    assert_eq!(
        keep(&store, &file, none).unwrap(),
        None,
        "nothing to keep yet"
    );

    write(&file, "a\n1\n", 0);
    keep(&store, &file, none).unwrap();
    write(&file, "a\n2\n", 1);
    keep(&store, &file, none).unwrap();

    let versions = list(&store, &file);
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
    assert!(list(&store, &tmp.path().join("other.csv")).is_empty());
}

#[test]
fn the_per_file_limit_drops_the_oldest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let store = Store::Shared(root.clone());
    let file = tmp.path().join("t.csv");
    let limits = Limits {
        per_file: Some(2),
        total_bytes: None,
    };
    for i in 0..4 {
        write(&file, &format!("v{i}"), i);
        keep(&store, &file, limits).unwrap();
    }
    let bodies: Vec<String> = list(&store, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["v3", "v2"]);
}

#[test]
fn the_size_limit_drops_the_oldest_but_keeps_each_files_newest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let store = Store::Shared(root.clone());
    let a = tmp.path().join("a.csv");
    let b = tmp.path().join("b.csv");
    let none = Limits::default();
    write(&a, "aaaaaaaaaa", 0); // 10 bytes each
    keep(&store, &a, none).unwrap();
    write(&b, "bbbbbbbbbb", 1);
    keep(&store, &b, none).unwrap();
    write(&a, "AAAAAAAAAA", 2);
    keep(&store, &a, none).unwrap();

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
    let a_left: Vec<String> = list(&store, &a)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(a_left, ["AAAAAAAAAA"]);
    assert_eq!(list(&store, &b).len(), 1);
}

#[test]
fn restore_puts_the_version_back_and_keeps_the_current_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let store = Store::Shared(root.clone());
    let file = tmp.path().join("t.csv");
    let none = Limits::default();
    write(&file, "old", 0);
    keep(&store, &file, none).unwrap();
    write(&file, "new", 1);

    let old = list(&store, &file)[0].path.clone();
    restore(&store, &old, &file, none).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    let bodies: Vec<String> = list(&store, &file)
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
    let store = Store::Shared(root.clone());
    let file = tmp.path().join("t.csv");
    let one = Limits {
        per_file: Some(1),
        total_bytes: None,
    };
    write(&file, "old", 0);
    keep(&store, &file, one).unwrap();
    write(&file, "new", 1);

    let old = list(&store, &file)[0].path.clone();
    restore(&store, &old, &file, one).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
    let bodies: Vec<String> = list(&store, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["new"], "the limit holds once the version is back");
}

#[test]
fn a_folder_with_no_versions_left_is_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("versions");
    let store = Store::Shared(root.clone());
    let file = tmp.path().join("t.csv");
    write(&file, "x", 0);
    keep(&store, &file, Limits::default()).unwrap();
    let dir = dir_for(&root, &file);
    std::fs::remove_file(&list(&store, &file)[0].path).unwrap();
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

#[test]
fn a_subfolder_beside_the_file_holds_its_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("sales.csv");
    let one = Limits {
        per_file: Some(1),
        total_bytes: None,
    };
    write(&file, "v1", 0);
    keep(&Store::Subfolder, &file, one).unwrap();
    write(&file, "v2", 1);
    let kept = keep(&Store::Subfolder, &file, one).unwrap().unwrap();
    let dir = tmp.path().join(SUBFOLDER).join("sales.csv");
    assert_eq!(kept.parent(), Some(dir.as_path()));
    assert!(file_name(&kept).ends_with("-sales.csv"), "{kept:?}");
    assert!(!dir.join(SOURCE_FILE).exists(), "the folder names the file");
    let bodies: Vec<String> = list(&Store::Subfolder, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["v2"], "the per-file limit holds there too");
}

#[test]
fn right_beside_the_file_only_its_own_versions_count() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("sales.csv");
    let other = tmp.path().join("sales2.csv");
    let none = Limits::default();
    write(&file, "v1", 0);
    let kept = keep(&Store::Beside, &file, none).unwrap().unwrap();
    assert_eq!(kept.parent(), file.parent());
    let name = file_name(&kept);
    assert!(
        name.starts_with("sales.") && name.ends_with(".csv"),
        "{name}"
    );
    write(&other, "o1", 1);
    keep(&Store::Beside, &other, none).unwrap();
    // Neither the file itself, nor the other file, nor its version, is one.
    assert_eq!(list(&Store::Beside, &file).len(), 1);
    assert_eq!(list(&Store::Beside, &other).len(), 1);

    // A limit drops only this file's oldest, never another file's.
    write(&file, "v2", 2);
    keep(
        &Store::Beside,
        &file,
        Limits {
            per_file: Some(1),
            total_bytes: None,
        },
    )
    .unwrap();
    let bodies: Vec<String> = list(&Store::Beside, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["v2"]);
    assert_eq!(list(&Store::Beside, &other).len(), 1);
}

#[test]
fn beside_names_round_trip_with_and_without_an_extension() {
    let stamp = "20261020-140211-123";
    for name in ["sales.csv", "Makefile", "data.tar.gz", ".env"] {
        for n in [0, 2] {
            let v = Store::Beside.version_name(name, stamp, n);
            assert_eq!(
                Store::Beside.stamp_of(&v, name).as_deref(),
                Some("2026-10-20 14:02:11"),
                "{v}"
            );
        }
        assert_eq!(Store::Beside.stamp_of(name, name), None);
    }
    // Not a version of `sales.csv`, though close.
    assert_eq!(
        Store::Beside.stamp_of("sales.20261020-140211-123.tsv", "sales.csv"),
        None
    );
    assert_eq!(
        Store::Beside.stamp_of("sales.20261020-140211-123.csv", "les.csv"),
        None
    );
}

#[test]
fn every_place_is_listed_newest_first() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("t.csv");
    let shared = Store::Shared(tmp.path().join("versions"));
    let none = Limits::default();
    write(&file, "in shared", 0);
    keep(&shared, &file, none).unwrap();
    write(&file, "beside", 1);
    keep(&Store::Beside, &file, none).unwrap();
    let stores = [Store::Beside, shared.clone(), Store::Subfolder];
    let bodies: Vec<String> = list_all(&stores, &file)
        .iter()
        .map(|v| std::fs::read_to_string(&v.path).unwrap())
        .collect();
    assert_eq!(bodies, ["beside", "in shared"]);
}
