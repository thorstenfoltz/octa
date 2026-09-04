//! Tests for `octa::data::unique_columns::find_unique_columns`.

use std::collections::HashMap;

use octa::data::unique_columns::{MAX_COMBO_SIZE, find_unique_columns};
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
        formulas: std::collections::HashMap::new(),
    }
}

#[test]
fn single_column_all_unique_no_nulls_flags_unique() {
    let t = build(
        &[("id", "Int64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Int(3)],
        ],
    );
    let a = find_unique_columns(&t, 1);
    assert_eq!(a.total_rows, 3);
    assert_eq!(a.single.len(), 1);
    let r = &a.single[0];
    assert_eq!(r.column, "id");
    assert_eq!(r.distinct_count, 3);
    assert_eq!(r.null_count, 0);
    assert!(r.is_unique);
    assert!(a.combos.is_empty());
}

#[test]
fn single_column_with_null_is_not_pk_candidate() {
    let t = build(
        &[("id", "Int64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Null],
        ],
    );
    let a = find_unique_columns(&t, 1);
    let r = &a.single[0];
    assert_eq!(r.null_count, 1);
    assert!(!r.is_unique, "null must disqualify PK candidacy");
}

#[test]
fn duplicate_values_disqualify() {
    let t = build(
        &[("a", "Int64")],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Int(1)],
        ],
    );
    let r = &find_unique_columns(&t, 1).single[0];
    assert_eq!(r.distinct_count, 2);
    assert!(!r.is_unique);
}

#[test]
fn combo_detects_compound_key() {
    // Neither column unique alone, but the pair is.
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
        ],
    );
    let a = find_unique_columns(&t, 2);
    // Neither column is unique alone.
    assert!(!a.single[0].is_unique);
    assert!(!a.single[1].is_unique);
    // The pair is.
    assert_eq!(a.combos.len(), 1);
    assert_eq!(a.combos[0].columns, vec!["first", "last"]);
    assert!(a.combos[0].is_unique);
}

#[test]
fn combo_size_clamped_to_max() {
    let t = build(
        &[
            ("a", "Int64"),
            ("b", "Int64"),
            ("c", "Int64"),
            ("d", "Int64"),
        ],
        vec![
            vec![
                CellValue::Int(1),
                CellValue::Int(1),
                CellValue::Int(1),
                CellValue::Int(1),
            ],
            vec![
                CellValue::Int(1),
                CellValue::Int(2),
                CellValue::Int(2),
                CellValue::Int(2),
            ],
            vec![
                CellValue::Int(2),
                CellValue::Int(1),
                CellValue::Int(2),
                CellValue::Int(3),
            ],
        ],
    );
    // Ask for 100 — should be clamped to MAX_COMBO_SIZE.
    let a = find_unique_columns(&t, 100);
    // Each combo size from 2..=MAX_COMBO_SIZE; verify no quadruples.
    for c in &a.combos {
        assert!(
            c.columns.len() <= MAX_COMBO_SIZE,
            "combo size exceeded clamp: {c:?}"
        );
    }
}

#[test]
fn already_unique_columns_skip_combos() {
    let t = build(
        &[("id", "Int64"), ("category", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::String("a".into())],
            vec![CellValue::Int(2), CellValue::String("a".into())],
            vec![CellValue::Int(3), CellValue::String("b".into())],
        ],
    );
    let a = find_unique_columns(&t, 2);
    // `id` is unique on its own; combos including `id` are skipped to
    // avoid redundant findings.
    assert!(a.single[0].is_unique);
    for c in &a.combos {
        assert!(
            !c.columns.contains(&"id".to_string()),
            "combo should skip already-unique column: {c:?}"
        );
    }
}

#[test]
fn empty_table_returns_empty() {
    let t = build(&[("id", "Int64")], vec![]);
    let a = find_unique_columns(&t, 2);
    assert_eq!(a.total_rows, 0);
    assert_eq!(a.single.len(), 1);
    assert_eq!(a.single[0].distinct_count, 0);
    // Empty table has no PK candidate (the function returns false when
    // total_rows == 0).
    assert!(!a.single[0].is_unique);
    assert!(a.combos.is_empty());
}

#[test]
fn null_only_column_is_not_unique() {
    let t = build(
        &[("notes", "Utf8")],
        vec![vec![CellValue::Null], vec![CellValue::Null]],
    );
    let r = &find_unique_columns(&t, 1).single[0];
    assert!(!r.is_unique);
    assert_eq!(r.null_count, 2);
}

#[test]
fn combo_size_one_means_no_combos() {
    let t = build(
        &[("a", "Int64"), ("b", "Int64")],
        vec![
            vec![CellValue::Int(1), CellValue::Int(1)],
            vec![CellValue::Int(1), CellValue::Int(2)],
        ],
    );
    let a = find_unique_columns(&t, 1);
    assert!(a.combos.is_empty());
}
