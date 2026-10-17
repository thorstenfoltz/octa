use super::*;
use octa::data::folder_compare::{FileEntry, FolderListing, compare_table};

fn listing(label: &str, files: &[(&str, u64)]) -> FolderListing {
    FolderListing {
        root: format!("/{label}"),
        label: label.into(),
        files: files
            .iter()
            .map(|(p, size)| FileEntry {
                rel_path: (*p).into(),
                size: *size,
                modified: None,
                hash: None,
            })
            .collect(),
        truncated: false,
    }
}

#[test]
fn status_counts_reads_different_and_missing_rows() {
    // same.csv: equal size, unhashed -> same; diff.csv: sizes differ;
    // only_a.csv / only_b.csv: one folder each -> missing.
    let table = compare_table(&[
        listing("a", &[("same.csv", 3), ("diff.csv", 1), ("only_a.csv", 5)]),
        listing("b", &[("same.csv", 3), ("diff.csv", 2), ("only_b.csv", 5)]),
    ]);
    assert_eq!(status_counts(&table), (1, 2));
}

#[test]
fn a_folder_is_added_once() {
    let mut st = crate::app::dialogs::folder_compare::FolderCompareState::new();
    let a = GuiFolderRoot::Local("/data/a".into());
    let b = GuiFolderRoot::Cloud {
        conn_id: "c1".into(),
        key: "x/".into(),
    };
    assert!(st.add_root(a.clone()));
    assert!(st.add_root(b.clone()));
    assert!(!st.add_root(a));
    assert_eq!(st.roots.len(), 2);
    assert!(st.recursive, "subfolders are included by default");
}

#[test]
fn only_known_extensions_can_be_compared_row_by_row() {
    let known = vec!["csv".to_string(), "parquet".to_string(), "log".to_string()];
    assert!(comparable("sub/sales.CSV", &known));
    assert!(comparable("x.parquet", &known));
    assert!(comparable("server.log", &known));
    assert!(!comparable("photo.jpg", &known));
    assert!(!comparable("README", &known));
    assert!(
        comparable("2026/sales.csv.gz", &known),
        "opens as the csv inside"
    );
    assert!(!comparable("photo.jpg.zst", &known));
}
