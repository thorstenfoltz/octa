//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;
use std::collections::HashMap;

fn empty_table() -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }],
        rows: vec![],
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
fn octopuses_egg_triggers_case_insensitively() {
    let t = empty_table();
    for q in [
        "SELECT * FROM octopuses",
        "select * from octopuses",
        "  SELECT   *   FROM   octopuses  ",
        "SELECT * FROM octopuses;",
    ] {
        let out = run_query(&t, q).expect(q);
        assert_eq!(out.kind, QueryKind::Select);
        assert_eq!(out.table.col_count(), 6);
        assert_eq!(out.table.row_count(), 5);
        assert_eq!(out.table.columns[0].name, "id");
        assert_eq!(out.table.columns[1].name, "name");
    }
}

#[test]
fn octopuses_egg_does_not_swallow_real_queries() {
    let t = empty_table();
    let err = run_query(&t, "SELECT * FROM octopuses WHERE iq > 100").unwrap_err();
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("octopuses"),
        "expected DuckDB to complain about missing table `octopuses`, got: {msg}"
    );
}

#[test]
fn h2o_egg_triggers_case_insensitively() {
    let t = empty_table();
    for q in [
        "SELECT * FROM h2o",
        "select * from h2o",
        "  SELECT   *   FROM   H2O  ",
        "SELECT * FROM h2o;",
    ] {
        let out = run_query(&t, q).expect(q);
        assert_eq!(out.kind, QueryKind::Select);
        assert_eq!(out.table.col_count(), 7);
        assert_eq!(out.table.row_count(), 10);
        assert_eq!(out.table.columns[0].name, "id");
        assert_eq!(out.table.columns[1].name, "zone");
        assert_eq!(out.table.columns[3].data_type, "Float64");
    }
}
