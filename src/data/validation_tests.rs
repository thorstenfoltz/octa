//! Unit tests for [`validation`](validation). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::{CellValue, ColumnInfo};

fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        columns: cols
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".to_string(),
            })
            .collect(),
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

#[test]
fn not_null_flags_blank_cells() {
    let t = table(
        &["a"],
        vec![
            vec![CellValue::String("x".into())],
            vec![CellValue::Null],
            vec![CellValue::String("  ".into())],
        ],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::NotNull,
        }],
    );
    assert_eq!(v, HashSet::from([(1, 0), (2, 0)]));
}

#[test]
fn range_flags_out_of_bounds_and_non_numeric() {
    let t = table(
        &["n"],
        vec![
            vec![CellValue::Int(5)],
            vec![CellValue::Int(15)],
            vec![CellValue::String("abc".into())],
            vec![CellValue::Null],
        ],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::Range {
                min: Some(0.0),
                max: Some(10.0),
            },
        }],
    );
    // 15 is above max, "abc" is non-numeric; null is ignored by Range.
    assert_eq!(v, HashSet::from([(1, 0), (2, 0)]));
}

#[test]
fn regex_flags_non_matching() {
    let t = table(
        &["code"],
        vec![
            vec![CellValue::String("AB12".into())],
            vec![CellValue::String("zz".into())],
        ],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::Regex("[A-Z]{2}[0-9]{2}".into()),
        }],
    );
    assert_eq!(v, HashSet::from([(1, 0)]));
}

#[test]
fn unique_flags_all_duplicated_cells() {
    let t = table(
        &["id"],
        vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Int(1)],
        ],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::Unique,
        }],
    );
    assert_eq!(v, HashSet::from([(0, 0), (2, 0)]));
}

#[test]
fn max_length_counts_chars() {
    let t = table(
        &["s"],
        vec![
            vec![CellValue::String("ok".into())],
            vec![CellValue::String("toolong".into())],
        ],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::MaxLength(3),
        }],
    );
    assert_eq!(v, HashSet::from([(1, 0)]));
}

#[test]
fn all_columns_when_column_is_none() {
    let t = table(
        &["a", "b"],
        vec![vec![CellValue::Null, CellValue::String("y".into())]],
    );
    let v = violations(
        &t,
        &[ValidationRule {
            column: None,
            kind: ValidationKind::NotNull,
        }],
    );
    assert_eq!(v, HashSet::from([(0, 0)]));
}

#[test]
fn invalid_regex_disables_rule() {
    let t = table(&["a"], vec![vec![CellValue::String("x".into())]]);
    let v = violations(
        &t,
        &[ValidationRule {
            column: Some(0),
            kind: ValidationKind::Regex("([".into()),
        }],
    );
    assert!(v.is_empty());
}
