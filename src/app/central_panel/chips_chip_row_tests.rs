use super::*;
use octa::data::{ColumnInfo, DataTable};
use std::collections::{HashMap, HashSet};

fn two_col_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "year".into(),
            data_type: "Int64".into(),
        },
    ];
    t
}

/// A chip has to say what it is doing without being read twice. One
/// allowed value reads as the value itself; several read as a count,
/// because a chip listing fifteen values is not a chip any more.
#[test]
fn chip_labels_name_the_column_and_the_value_count() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(0, ["Aachen".to_string()].into_iter().collect());
    filters.insert(
        1,
        ["2025".to_string(), "2026".to_string()]
            .into_iter()
            .collect(),
    );

    let labels = column_filter_chip_labels(&t, &filters);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0], (0, "city: Aachen".to_string()));
    assert_eq!(labels[1].0, 1);
    assert!(
        labels[1].1.starts_with("year: "),
        "names the column: {}",
        labels[1].1
    );
    assert!(
        labels[1].1.contains('2'),
        "several values report a count: {}",
        labels[1].1
    );
}

/// Chips come back in column order whatever order the map iterates, or
/// the row would reshuffle itself between frames.
#[test]
fn chips_are_ordered_by_column_not_by_hash() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(1, ["2025".to_string()].into_iter().collect());
    filters.insert(0, ["Aachen".to_string()].into_iter().collect());
    let labels = column_filter_chip_labels(&t, &filters);
    assert_eq!(
        labels.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

/// A filter on a column that no longer exists (deleted while filtered)
/// must be skipped, not panic and not render a nameless chip.
#[test]
fn a_filter_on_a_missing_column_is_skipped() {
    let t = two_col_table();
    let mut filters: HashMap<usize, HashSet<String>> = HashMap::new();
    filters.insert(9, ["x".to_string()].into_iter().collect());
    assert!(column_filter_chip_labels(&t, &filters).is_empty());
}
