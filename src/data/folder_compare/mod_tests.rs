use super::*;

fn f(path: &str, size: u64, hash: Option<u8>) -> FileEntry {
    FileEntry {
        rel_path: path.into(),
        size,
        modified: None,
        hash: hash.map(|b| [b; 32]),
    }
}

fn listing(label: &str, files: Vec<FileEntry>) -> FolderListing {
    FolderListing {
        root: format!("/{label}"),
        label: label.into(),
        files,
        truncated: false,
    }
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t
        .columns
        .iter()
        .position(|c| c.name == col)
        .unwrap_or_else(|| panic!("no column {col}"));
    t.get(row, c).map(|v| v.to_string()).unwrap_or_default()
}

#[test]
fn labels_are_folder_names_grown_until_unique() {
    let roots: Vec<String> = ["/a/data", "/b/data", "/b/other"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(labels_for(&roots), ["a/data", "b/data", "other"]);
    let roots: Vec<String> = ["s3://bucket/x/", "/home/u/x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(labels_for(&roots), ["bucket/x", "u/x"]);
    let roots: Vec<String> = ["/a", "/a"].iter().map(|s| s.to_string()).collect();
    assert_eq!(labels_for(&roots), ["a", "a (2)"]);
}

#[test]
fn only_equal_sizes_in_two_folders_need_a_hash() {
    let l = [
        listing(
            "A",
            vec![
                f("a.csv", 10, None),
                f("b.csv", 5, None),
                f("only.csv", 3, None),
            ],
        ),
        listing("B", vec![f("a.csv", 10, None), f("b.csv", 6, None)]),
    ];
    assert_eq!(
        which_need_hash(&l),
        [(0, "a.csv".to_string()), (1, "a.csv".to_string())]
    );
}

#[test]
fn statuses_for_two_folders() {
    let l = [
        listing(
            "A",
            vec![
                f("same.csv", 10, Some(1)),
                f("size.csv", 5, None),
                f("gone.csv", 3, None),
                f("content.csv", 7, Some(1)),
            ],
        ),
        listing(
            "B",
            vec![
                f("same.csv", 10, Some(1)),
                f("size.csv", 6, None),
                f("content.csv", 7, Some(2)),
            ],
        ),
    ];
    let t = compare_table(&l);
    let status: Vec<(String, String)> = (0..t.row_count())
        .map(|r| (cell(&t, r, "path"), cell(&t, r, "status")))
        .collect();
    assert_eq!(
        status,
        [
            ("content.csv".to_string(), "different".to_string()),
            ("gone.csv".to_string(), "missing".to_string()),
            ("same.csv".to_string(), "same".to_string()),
            ("size.csv".to_string(), "different".to_string()),
        ]
    );
    assert_eq!(cell(&t, 1, "in"), "A");
    assert_eq!(cell(&t, 1, "missing from"), "B");
    assert!(
        cell(&t, 1, "size: B").is_empty(),
        "absent copies have no size"
    );
    assert!(any_difference(&t));
}

#[test]
fn with_three_folders_the_hash_columns_show_which_copies_match() {
    let l = [
        listing("A", vec![f("x.csv", 4, Some(1))]),
        listing("B", vec![f("x.csv", 4, Some(1))]),
        listing("C", vec![f("x.csv", 4, Some(9))]),
    ];
    let t = compare_table(&l);
    assert_eq!(cell(&t, 0, "status"), "different");
    assert_eq!(cell(&t, 0, "hash: A"), cell(&t, 0, "hash: B"));
    assert_ne!(cell(&t, 0, "hash: A"), cell(&t, 0, "hash: C"));
    assert_eq!(cell(&t, 0, "hash: A").len(), 64, "full BLAKE3 hex");
}

#[test]
fn everything_equal_is_no_difference() {
    let l = [
        listing("A", vec![f("x.csv", 4, Some(1))]),
        listing("B", vec![f("x.csv", 4, Some(1))]),
    ];
    assert!(!any_difference(&compare_table(&l)));
}

#[test]
fn pairs_cover_the_folders_holding_the_file() {
    let l = [
        listing("A", vec![f("x.csv", 4, Some(1))]),
        listing("B", vec![]),
        listing("C", vec![f("x.csv", 5, None)]),
    ];
    let t = compare_table(&l);
    let labels: Vec<String> = l.iter().map(|x| x.label.clone()).collect();
    assert_eq!(pairs_for_row(&t, 0, &labels), [(0, 2)]);
}
