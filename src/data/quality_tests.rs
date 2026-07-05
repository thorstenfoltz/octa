//! Unit tests for [`quality`](quality). Split out and included via `#[path]`
//! so it stays an inner `tests` module with access to private items.

use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table() -> DataTable {
    // Two columns: an all-present numeric with an outlier, and a text col with a null.
    let columns = vec![
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "email".into(),
            data_type: "Utf8".into(),
        },
    ];
    let rows = vec![
        vec![CellValue::Int(10), CellValue::String("a@b.com".into())],
        vec![CellValue::Int(11), CellValue::String("c@d.com".into())],
        vec![CellValue::Int(12), CellValue::Null],
        vec![CellValue::Int(13), CellValue::String("e@f.com".into())],
        vec![CellValue::Int(9000), CellValue::String("g@h.com".into())],
    ];
    DataTable {
        columns,
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
fn one_row_per_source_column() {
    let rep = build_quality_report(&table()).unwrap();
    assert_eq!(rep.table.row_count(), 2);
}

#[test]
fn headers_are_snake_case_ids() {
    let rep = build_quality_report(&table()).unwrap();
    let names: Vec<&str> = rep.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"null_percentage"));
    assert!(names.contains(&"score"));
    assert!(names.contains(&"pii_flag"));
}

#[test]
fn null_percentage_reflects_missing_cell() {
    let rep = build_quality_report(&table()).unwrap();
    // email column (report row index 1) has 1/5 null = 20%.
    let col = rep
        .table
        .columns
        .iter()
        .position(|c| c.name == "null_percentage")
        .unwrap();
    let email_row = 1;
    let v = rep.table.get(email_row, col).unwrap();
    assert_eq!(v.to_string(), "20"); // rounded whole percent
}

#[test]
fn overall_score_in_range() {
    let rep = build_quality_report(&table()).unwrap();
    assert!(rep.overall_score >= 0.0 && rep.overall_score <= 100.0);
}

#[test]
fn hint_keys_align_with_ids() {
    assert_eq!(quality_column_ids().len(), quality_column_hint_keys().len());
}
