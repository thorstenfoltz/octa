//! Unit tests for [`workspace`](workspace). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;

fn simple_table(name_col: &str, name_val: &str, score: f64) -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: name_col.into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![vec![
            CellValue::Int(1),
            CellValue::String(name_val.into()),
            CellValue::Float(score),
        ]],
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
fn dedupe_appends_suffix() {
    let existing: std::collections::HashSet<String> =
        ["customers".to_string()].into_iter().collect();
    let name = dedupe_sql_name("Customers", |s| existing.contains(s));
    assert_eq!(name, "customers_2");
}

#[test]
fn sanitize_replaces_unsafe_chars() {
    assert_eq!(sanitize_sql_name("My File 2024.csv"), "my_file_2024_csv");
    assert_eq!(sanitize_sql_name("2024 data"), "t_2024_data");
    assert_eq!(sanitize_sql_name("___"), "table");
}

#[test]
fn workspace_round_trips_single_table() {
    let mut ws = SqlWorkspace::new().unwrap();
    let t = simple_table("name", "Alice", 9.5);
    ws.set_active_table(&t).unwrap();
    let out = ws.execute("SELECT id, score FROM data").unwrap();
    assert_eq!(out.kind, QueryKind::Select);
    assert_eq!(out.table.row_count(), 1);
    assert_eq!(out.table.col_count(), 2);
}

#[test]
fn workspace_supports_join_across_two_tables() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&simple_table("name", "Alice", 9.5))
        .unwrap();
    ws.add_table(
        "extra",
        &simple_table("label", "Tier-1", 99.0),
        TableOrigin::TabClone("extra".into()),
    )
    .unwrap();
    let out = ws
        .execute("SELECT d.name, e.label FROM data d JOIN extra e ON d.id = e.id")
        .unwrap();
    assert_eq!(out.table.row_count(), 1);
    assert_eq!(out.table.col_count(), 2);
}

#[test]
fn add_table_replaces_existing_registration() {
    let mut ws = SqlWorkspace::new().unwrap();
    ws.add_table(
        "foo",
        &simple_table("name", "v1", 1.0),
        TableOrigin::ActiveTab,
    )
    .unwrap();
    ws.add_table(
        "foo",
        &simple_table("name", "v2", 2.0),
        TableOrigin::ActiveTab,
    )
    .unwrap();
    let out = ws.execute("SELECT name FROM foo").unwrap();
    assert_eq!(out.table.get(0, 0).unwrap().to_string(), "v2");
}
