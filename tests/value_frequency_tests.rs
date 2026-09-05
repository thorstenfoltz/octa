//! Tests for `octa::data::value_frequency::compute_value_frequency`.
//! Pure function tests — no GUI. End-to-end dialog wiring is verified
//! by manual smoke test.

use std::collections::HashMap;

use octa::data::value_frequency::{BinningMode, compute_value_frequency};
use octa::data::{CellValue, ColumnInfo, DataTable};

fn table_with_column(col_type: &str, cells: Vec<CellValue>) -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "x".into(),
            data_type: col_type.into(),
        }],
        rows: cells.into_iter().map(|c| vec![c]).collect(),
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
        formulas: std::collections::HashMap::new(),
    }
}

#[test]
fn out_of_range_column_returns_none() {
    let t = table_with_column("Utf8", vec![CellValue::String("a".into())]);
    assert!(compute_value_frequency(&t, 99, None, BinningMode::None).is_none());
}

#[test]
fn string_column_counts_each_value() {
    let t = table_with_column(
        "Utf8",
        vec![
            CellValue::String("apple".into()),
            CellValue::String("banana".into()),
            CellValue::String("apple".into()),
            CellValue::String("cherry".into()),
            CellValue::String("apple".into()),
        ],
    );
    let r = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
    assert_eq!(r.column_name, "x");
    assert_eq!(r.rows.len(), 3);
    assert_eq!(r.rows[0].label, "apple");
    assert_eq!(r.rows[0].count, 3);
    assert_eq!(r.rows[1].count, 1);
    assert_eq!(r.rows[2].count, 1);
    // Ties broken alphabetically.
    assert_eq!(r.rows[1].label, "banana");
    assert_eq!(r.rows[2].label, "cherry");
    assert_eq!(r.nulls, 0);
    assert_eq!(r.total_non_null, 5);
    assert_eq!(r.unique_count, 3);
    assert!(!r.binned);
}

#[test]
fn nulls_counted_separately_from_rows() {
    let t = table_with_column(
        "Utf8",
        vec![
            CellValue::String("a".into()),
            CellValue::Null,
            CellValue::String("a".into()),
            CellValue::String("".into()), // empty string treated as null
        ],
    );
    let r = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].label, "a");
    assert_eq!(r.rows[0].count, 2);
    assert_eq!(r.nulls, 2);
    assert_eq!(r.total_non_null, 2);
    assert_eq!(r.unique_count, 1);
}

#[test]
fn top_n_truncates_but_preserves_aggregates() {
    let mut cells = Vec::new();
    for _ in 0..5 {
        cells.push(CellValue::String("a".into()));
    }
    for _ in 0..3 {
        cells.push(CellValue::String("b".into()));
    }
    cells.push(CellValue::String("c".into()));
    cells.push(CellValue::String("d".into()));
    let t = table_with_column("Utf8", cells);
    let r = compute_value_frequency(&t, 0, Some(2), BinningMode::None).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert_eq!(r.rows[0].label, "a");
    assert_eq!(r.rows[1].label, "b");
    // unique_count is the full distinct count, not the truncated count.
    assert_eq!(r.unique_count, 4);
    assert_eq!(r.total_non_null, 10);
}

#[test]
fn integer_column_without_binning_shows_raw() {
    let t = table_with_column(
        "Int64",
        vec![
            CellValue::Int(1),
            CellValue::Int(2),
            CellValue::Int(2),
            CellValue::Int(3),
            CellValue::Int(3),
            CellValue::Int(3),
        ],
    );
    let r = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
    assert!(!r.binned);
    assert_eq!(r.rows[0].label, "3");
    assert_eq!(r.rows[0].count, 3);
    assert_eq!(r.rows[1].count, 2);
    assert_eq!(r.rows[2].count, 1);
}

#[test]
fn integer_column_with_sturges_bins_groups_into_ranges() {
    let cells: Vec<CellValue> = (1..=100).map(CellValue::Int).collect();
    let t = table_with_column("Int64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Sturges).unwrap();
    assert!(r.binned);
    // Sturges for n=100: ceil(1 + log2(100)) = ceil(7.64) = 8 bins.
    assert_eq!(r.rows.len(), 8);
    // Every label should look like "[lo, hi)" or "[lo, hi]".
    for row in &r.rows {
        assert!(row.label.starts_with('['));
        let last = row.label.chars().last().unwrap();
        assert!(last == ')' || last == ']');
    }
    // Total bin counts equal the row count.
    let total: usize = r.rows.iter().map(|r| r.count).sum();
    assert_eq!(total, 100);
}

#[test]
fn integer_column_with_custom_bin_count() {
    let cells: Vec<CellValue> = (1..=100).map(CellValue::Int).collect();
    let t = table_with_column("Int64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Custom(5)).unwrap();
    assert!(r.binned);
    // Exactly 5 bins (all non-empty for a uniform 1..=100 spread).
    assert_eq!(r.rows.len(), 5);
    let total: usize = r.rows.iter().map(|r| r.count).sum();
    assert_eq!(total, 100);
}

#[test]
fn custom_bins_keep_empty_buckets_in_range_order() {
    // Clustered data: two values at 0, one at 100. With 10 bins the result is
    // 10 rows (one per range, most empty) in ascending range order - NOT a
    // collapsed "only occupied buckets" list.
    let cells = vec![CellValue::Int(0), CellValue::Int(0), CellValue::Int(100)];
    let t = table_with_column("Int64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Custom(10)).unwrap();
    assert!(r.binned);
    assert_eq!(r.rows.len(), 10, "requesting 10 bins yields 10 rows");
    // First bin holds the two zeros; last bin holds the 100; middle bins empty.
    assert_eq!(r.rows[0].count, 2);
    assert_eq!(r.rows[9].count, 1);
    let middle_total: usize = r.rows[1..9].iter().map(|row| row.count).sum();
    assert_eq!(middle_total, 0, "middle bins are empty but still present");
    let total: usize = r.rows.iter().map(|row| row.count).sum();
    assert_eq!(total, 3);
}

#[test]
fn custom_bin_count_clamped_to_at_least_one() {
    let cells: Vec<CellValue> = (1..=10).map(CellValue::Int).collect();
    let t = table_with_column("Int64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Custom(0)).unwrap();
    assert!(r.binned);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].count, 10);
}

#[test]
fn binning_with_all_equal_values_collapses_to_one_bucket() {
    let cells: Vec<CellValue> = (0..20).map(|_| CellValue::Int(42)).collect();
    let t = table_with_column("Int64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Sturges).unwrap();
    assert!(r.binned);
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].count, 20);
}

#[test]
fn binning_on_string_column_is_ignored() {
    let t = table_with_column(
        "Utf8",
        vec![
            CellValue::String("a".into()),
            CellValue::String("a".into()),
            CellValue::String("b".into()),
        ],
    );
    let r = compute_value_frequency(&t, 0, None, BinningMode::Sturges).unwrap();
    assert!(!r.binned, "string columns never bin");
    assert_eq!(r.rows.len(), 2);
}

#[test]
fn boolean_column_works() {
    let t = table_with_column(
        "Boolean",
        vec![
            CellValue::Bool(true),
            CellValue::Bool(false),
            CellValue::Bool(true),
            CellValue::Bool(true),
        ],
    );
    let r = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
    assert_eq!(r.rows[0].label, "true");
    assert_eq!(r.rows[0].count, 3);
    assert_eq!(r.rows[1].label, "false");
    assert_eq!(r.rows[1].count, 1);
}

#[test]
fn empty_column_returns_empty_result() {
    let t = table_with_column("Utf8", vec![]);
    let r = compute_value_frequency(&t, 0, None, BinningMode::None).unwrap();
    assert!(r.rows.is_empty());
    assert_eq!(r.nulls, 0);
    assert_eq!(r.total_non_null, 0);
    assert_eq!(r.unique_count, 0);
}

#[test]
fn floats_with_binning_handle_nan_separately() {
    let cells: Vec<CellValue> = (1..=10)
        .map(|i| CellValue::Float(i as f64))
        .chain([CellValue::Float(f64::NAN), CellValue::Float(f64::INFINITY)])
        .collect();
    let t = table_with_column("Float64", cells);
    let r = compute_value_frequency(&t, 0, None, BinningMode::Sturges).unwrap();
    let labels: Vec<&str> = r.rows.iter().map(|x| x.label.as_str()).collect();
    assert!(labels.contains(&"NaN"));
    assert!(labels.contains(&"+Inf"));
    // The finite values still binned.
    let bin_count: usize = r
        .rows
        .iter()
        .filter(|x| x.label.starts_with('['))
        .map(|x| x.count)
        .sum();
    assert_eq!(bin_count, 10);
}
