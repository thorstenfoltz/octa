//! Unit tests for [`trim`](trim). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;

fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

#[test]
fn trims_leading_and_trailing() {
    let mut t = table(
        &["a", "b"],
        vec![vec![
            CellValue::String("  hi  ".into()),
            CellValue::String("ok".into()),
        ]],
    );
    let changed = trim_string_columns(&mut t);
    assert_eq!(changed, vec!["a".to_string()]);
    assert_eq!(t.rows[0][0], CellValue::String("hi".into()));
    assert_eq!(t.rows[0][1], CellValue::String("ok".into()));
}

#[test]
fn preserves_interior_whitespace() {
    let mut t = table(&["a"], vec![vec![CellValue::String("  a  b  ".into())]]);
    let changed = trim_string_columns(&mut t);
    assert_eq!(changed, vec!["a".to_string()]);
    assert_eq!(t.rows[0][0], CellValue::String("a  b".into()));
}

#[test]
fn leaves_non_string_cells() {
    let mut t = table(
        &["n", "s"],
        vec![vec![CellValue::Int(5), CellValue::String("x".into())]],
    );
    let changed = trim_string_columns(&mut t);
    assert!(changed.is_empty());
    assert_eq!(t.rows[0][0], CellValue::Int(5));
}

#[test]
fn reports_each_affected_column() {
    let mut t = table(
        &["a", "b", "c"],
        vec![
            vec![
                CellValue::String("x ".into()),
                CellValue::String("y".into()),
                CellValue::String(" z".into()),
            ],
            vec![
                CellValue::String("p".into()),
                CellValue::String("q".into()),
                CellValue::String("r".into()),
            ],
        ],
    );
    let changed = trim_string_columns(&mut t);
    assert_eq!(changed, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn clean_headers_snake_cases_and_decollides() {
    let mut t = table(
        &["First Name", "first-name", "  E-mail Address ", "Name"],
        vec![],
    );
    let changed = clean_headers(&mut t);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    // "First Name" and "first-name" both snake_case to "first_name"; the
    // second is de-collided to "first_name_2".
    assert_eq!(
        names,
        vec!["first_name", "first_name_2", "e_mail_address", "name"]
    );
    // Every title differed from its original (including "Name" -> "name"), so
    // all four are reported as changed.
    assert_eq!(
        changed,
        vec!["first_name", "first_name_2", "e_mail_address", "name"]
    );
}

#[test]
fn clean_headers_leaves_already_clean_titles() {
    let mut t = table(&["id", "first_name"], vec![]);
    let changed = clean_headers(&mut t);
    assert!(changed.is_empty());
}

#[test]
fn clean_headers_empty_title_falls_back() {
    let mut t = table(&["???", "ok"], vec![]);
    clean_headers(&mut t);
    assert_eq!(t.columns[0].name, "column");
    assert_eq!(t.columns[1].name, "ok");
}

#[test]
fn undo_log_restores_original_values() {
    let mut t = table(
        &[" a ", "b"],
        vec![
            vec![
                CellValue::String("  hi  ".into()),
                CellValue::String("ok".into()),
            ],
            vec![
                CellValue::String("x".into()),
                CellValue::String(" y ".into()),
            ],
        ],
    );
    let (changed, undo) = trim_string_columns_tracked(&mut t);
    assert!(!changed.is_empty());
    assert!(!undo.is_empty());
    // Trim happened.
    assert_eq!(t.columns[0].name, "a");
    assert_eq!(t.rows[0][0], CellValue::String("hi".into()));
    assert_eq!(t.rows[1][1], CellValue::String("y".into()));
    // Replay the undo log by hand and confirm it restores the originals.
    for (col_idx, title) in &undo.titles {
        t.columns[*col_idx].name = title.clone();
    }
    for (col_idx, cells) in &undo.cells {
        for (row_idx, val) in cells {
            t.rows[*row_idx][*col_idx] = CellValue::String(val.clone());
        }
    }
    assert_eq!(t.columns[0].name, " a ");
    assert_eq!(t.rows[0][0], CellValue::String("  hi  ".into()));
    assert_eq!(t.rows[1][1], CellValue::String(" y ".into()));
}
