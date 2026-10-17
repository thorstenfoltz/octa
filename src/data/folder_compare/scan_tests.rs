use super::*;
use std::sync::atomic::AtomicBool;

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    dir
}

fn sources(dirs: &[&tempfile::TempDir]) -> Vec<(String, FolderSource)> {
    dirs.iter()
        .map(|d| {
            (
                d.path().to_string_lossy().into_owned(),
                FolderSource::Local(d.path().to_path_buf()),
            )
        })
        .collect()
}

fn run(
    dirs: &[&tempfile::TempDir],
    recursive: bool,
    cap: usize,
) -> anyhow::Result<(DataTable, Vec<FolderListing>)> {
    compare_sources(
        sources(dirs),
        recursive,
        cap,
        &AtomicBool::new(false),
        &mut |_| {},
    )
}

fn status_of(t: &DataTable, path: &str) -> String {
    let r = (0..t.row_count())
        .find(|&r| t.get(r, 0).map(|v| v.to_string()) == Some(path.into()))
        .unwrap();
    t.get(r, 1).unwrap().to_string()
}

#[test]
fn identical_trees_are_the_same() {
    let a = tree(&[("x.csv", "1,2\n"), ("sub/y.csv", "3\n")]);
    let b = tree(&[("x.csv", "1,2\n"), ("sub/y.csv", "3\n")]);
    let (t, listings) = run(&[&a, &b], true, 1000).unwrap();
    assert_eq!(t.row_count(), 2);
    assert!(!crate::data::folder_compare::any_difference(&t));
    // `same` rests on the hashes: every equal-size copy was read.
    assert!(
        listings
            .iter()
            .flat_map(|l| &l.files)
            .all(|f| f.hash.is_some())
    );
}

#[test]
fn equal_size_different_content_is_caught_by_the_hash() {
    let a = tree(&[("x.csv", "abc")]);
    let b = tree(&[("x.csv", "abd")]);
    let (t, _) = run(&[&a, &b], true, 1000).unwrap();
    assert_eq!(status_of(&t, "x.csv"), "different");
}

#[test]
fn a_file_in_one_folder_is_missing_and_never_read() {
    let a = tree(&[("x.csv", "1"), ("only.csv", "2")]);
    let b = tree(&[("x.csv", "1")]);
    let (t, listings) = run(&[&a, &b], true, 1000).unwrap();
    assert_eq!(status_of(&t, "only.csv"), "missing");
    let only = listings[0]
        .files
        .iter()
        .find(|f| f.rel_path == "only.csv")
        .unwrap();
    assert!(only.hash.is_none());
}

#[test]
fn without_recursion_subfolders_are_skipped() {
    let a = tree(&[("x.csv", "1"), ("sub/y.csv", "2")]);
    let b = tree(&[("x.csv", "1")]);
    let (t, _) = run(&[&a, &b], false, 1000).unwrap();
    assert_eq!(t.row_count(), 1);
}

#[test]
fn the_file_limit_marks_the_listing_truncated() {
    let a = tree(&[("1.csv", "1"), ("2.csv", "2"), ("3.csv", "3")]);
    let b = tree(&[("1.csv", "1")]);
    let (_, listings) = run(&[&a, &b], true, 2).unwrap();
    assert!(listings[0].truncated);
    assert!(!listings[1].truncated);
}

#[test]
fn one_folder_is_not_a_comparison() {
    let a = tree(&[("x.csv", "1")]);
    assert!(run(&[&a], true, 1000).is_err());
}

#[test]
fn cloud_prefixes_end_in_a_slash() {
    assert_eq!(cloud_prefix(""), "");
    assert_eq!(cloud_prefix("data/2024"), "data/2024/");
    assert_eq!(cloud_prefix("data/2024/"), "data/2024/");
}

// `compare_table` reads two unhashed copies of equal size as `same`, so a
// half-hashed listing must never reach it: cancelling or a failed read
// ends the scan with an error, not a table.
#[test]
fn cancelling_mid_hash_builds_no_table() {
    let a = tree(&[("x.csv", "abc"), ("y.csv", "1")]);
    let b = tree(&[("x.csv", "abd"), ("y.csv", "1")]);
    let cancel = AtomicBool::new(false);
    let out = compare_sources(sources(&[&a, &b]), true, 1000, &cancel, &mut |p| {
        if matches!(p, ScanProgress::Hashing { .. }) {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(out.is_err());
}

#[test]
fn a_file_that_cannot_be_read_fails_the_scan() {
    let a = tree(&[("x.csv", "abc")]);
    let b = tree(&[("x.csv", "abd")]);
    let gone = a.path().join("x.csv");
    // Folder `a` is already listed when `b`'s listing starts.
    let mut seen = 0;
    let out = compare_sources(
        sources(&[&a, &b]),
        true,
        1000,
        &AtomicBool::new(false),
        &mut |p| {
            if matches!(p, ScanProgress::Listing(_)) {
                seen += 1;
                if seen == 2 {
                    std::fs::remove_file(&gone).unwrap();
                }
            }
        },
    );
    assert!(out.is_err());
}
