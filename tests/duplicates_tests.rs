//! Tests for `octa::data::duplicates::find_duplicate_rows`.

use std::collections::HashMap;

use octa::data::duplicates::find_duplicate_rows;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn build(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        columns: columns
            .iter()
            .map(|(n, t)| ColumnInfo {
                name: n.to_string(),
                data_type: t.to_string(),
            })
            .collect(),
        rows,
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
    }
}

#[test]
fn empty_table_returns_empty() {
    let t = build(&[("a", "Utf8")], vec![]);
    assert!(find_duplicate_rows(&t, &[0]).is_empty());
}

#[test]
fn empty_key_cols_returns_empty() {
    let t = build(
        &[("a", "Utf8")],
        vec![
            vec![CellValue::String("x".into())],
            vec![CellValue::String("x".into())],
        ],
    );
    assert!(find_duplicate_rows(&t, &[]).is_empty());
}

#[test]
fn out_of_range_cols_are_skipped() {
    let t = build(
        &[("a", "Utf8")],
        vec![
            vec![CellValue::String("x".into())],
            vec![CellValue::String("x".into())],
        ],
    );
    // The valid column index 0 is still picked up alongside the bogus 99.
    let dups = find_duplicate_rows(&t, &[99, 0]);
    assert_eq!(dups, vec![0, 1]);
}

#[test]
fn all_out_of_range_returns_empty() {
    let t = build(
        &[("a", "Utf8")],
        vec![vec![CellValue::String("x".into())]; 3],
    );
    assert!(find_duplicate_rows(&t, &[99, 100]).is_empty());
}

#[test]
fn single_column_key() {
    let t = build(
        &[("name", "Utf8")],
        vec![
            vec![CellValue::String("alice".into())],
            vec![CellValue::String("bob".into())],
            vec![CellValue::String("alice".into())],
            vec![CellValue::String("carol".into())],
            vec![CellValue::String("alice".into())],
        ],
    );
    assert_eq!(find_duplicate_rows(&t, &[0]), vec![0, 2, 4]);
}

#[test]
fn unique_rows_return_empty() {
    let t = build(
        &[("a", "Int64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Int(3)],
        ],
    );
    assert!(find_duplicate_rows(&t, &[0]).is_empty());
}

#[test]
fn multi_column_key_means_all_must_match() {
    let t = build(
        &[("first", "Utf8"), ("last", "Utf8")],
        vec![
            vec![
                CellValue::String("alice".into()),
                CellValue::String("smith".into()),
            ],
            vec![
                CellValue::String("alice".into()),
                CellValue::String("jones".into()),
            ],
            vec![
                CellValue::String("bob".into()),
                CellValue::String("smith".into()),
            ],
            vec![
                CellValue::String("alice".into()),
                CellValue::String("smith".into()),
            ],
        ],
    );
    // Only rows 0 and 3 share both first AND last.
    assert_eq!(find_duplicate_rows(&t, &[0, 1]), vec![0, 3]);
    // First-only: rows 0, 1, 3 all share "alice".
    assert_eq!(find_duplicate_rows(&t, &[0]), vec![0, 1, 3]);
    // Last-only: rows 0, 2, 3 all share "smith".
    assert_eq!(find_duplicate_rows(&t, &[1]), vec![0, 2, 3]);
}

#[test]
fn null_and_empty_string_treated_consistently_per_to_string() {
    // CellValue::Null and CellValue::String("") both render to "" via
    // Display; they collide as duplicate keys. The dedup function is
    // text-based, so this is expected — document it via the test.
    let t = build(
        &[("x", "Utf8")],
        vec![
            vec![CellValue::Null],
            vec![CellValue::String("".into())],
            vec![CellValue::String("foo".into())],
        ],
    );
    assert_eq!(find_duplicate_rows(&t, &[0]), vec![0, 1]);
}

#[test]
fn key_ordering_matters() {
    // Hashing "ab"+SEP+"cd" must not collide with "abc"+SEP+"d". The unit
    // separator between cells prevents adjacent-cell-merge collisions.
    let t = build(
        &[("a", "Utf8"), ("b", "Utf8")],
        vec![
            vec![
                CellValue::String("ab".into()),
                CellValue::String("cd".into()),
            ],
            vec![
                CellValue::String("abc".into()),
                CellValue::String("d".into()),
            ],
        ],
    );
    assert!(find_duplicate_rows(&t, &[0, 1]).is_empty());
}

#[test]
fn result_is_sorted_ascending() {
    let t = build(
        &[("a", "Utf8")],
        vec![
            vec![CellValue::String("x".into())],
            vec![CellValue::String("y".into())],
            vec![CellValue::String("z".into())],
            vec![CellValue::String("x".into())],
            vec![CellValue::String("z".into())],
        ],
    );
    let dups = find_duplicate_rows(&t, &[0]);
    let mut sorted = dups.clone();
    sorted.sort_unstable();
    assert_eq!(dups, sorted);
    assert_eq!(dups, vec![0, 2, 3, 4]);
}

#[test]
fn handles_mixed_types_via_display() {
    // Int(1) and Float(1.0) render differently ("1" vs "1.0") so they
    // don't collide — text-based dedup is type-sensitive in that sense.
    let t = build(
        &[("v", "Float64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Float(1.0)],
            vec![CellValue::Int(1)],
        ],
    );
    assert_eq!(find_duplicate_rows(&t, &[0]), vec![0, 2]);
}
