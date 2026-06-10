use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::sql::{QueryKind, run_query};
use std::collections::HashMap;

fn sample_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::String("Alice".into()),
                CellValue::Float(9.5),
            ],
            vec![
                CellValue::Int(2),
                CellValue::String("Bob".into()),
                CellValue::Float(7.0),
            ],
            vec![
                CellValue::Int(3),
                CellValue::String("Charlie".into()),
                CellValue::Float(8.25),
            ],
        ],
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
fn test_select_all_returns_all_rows() {
    let table = sample_table();
    let result = run_query(&table, "SELECT * FROM data ORDER BY id")
        .unwrap()
        .table;
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.col_count(), 3);
    assert_eq!(result.get(0, 1), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_timestamp_result_is_formatted_not_raw_epoch() {
    // Regression: SQL results used to surface TIMESTAMP / DATE columns as the
    // raw epoch integer (e.g. "1769975775172766") instead of a readable value.
    let table = sample_table();
    let result = run_query(
        &table,
        "SELECT TIMESTAMP '2026-02-01 19:56:15.172766' AS ts, DATE '2026-02-01' AS d",
    )
    .unwrap()
    .table;
    match result.get(0, 0) {
        Some(CellValue::DateTime(s)) => {
            assert!(s.starts_with("2026-02-01 19:56:15"), "got {s:?}");
            assert!(!s.chars().all(|c| c.is_ascii_digit()), "raw epoch: {s:?}");
        }
        other => panic!("expected DateTime, got {other:?}"),
    }
    assert_eq!(
        result.get(0, 1),
        Some(&CellValue::Date("2026-02-01".into()))
    );
}

#[test]
fn test_count_returns_scalar() {
    let table = sample_table();
    let result = run_query(&table, "SELECT COUNT(*) AS n FROM data")
        .unwrap()
        .table;
    assert_eq!(result.row_count(), 1);
    assert_eq!(result.col_count(), 1);
    assert_eq!(result.columns[0].name, "n");
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(3)));
}

#[test]
fn test_where_filter() {
    let table = sample_table();
    let result = run_query(&table, "SELECT name FROM data WHERE id > 1 ORDER BY id")
        .unwrap()
        .table;
    assert_eq!(result.row_count(), 2);
    assert_eq!(result.get(0, 0), Some(&CellValue::String("Bob".into())));
    assert_eq!(result.get(1, 0), Some(&CellValue::String("Charlie".into())));
}

#[test]
fn test_projection_preserves_aliases() {
    let table = sample_table();
    let result = run_query(
        &table,
        "SELECT id AS user_id, score * 10 AS scaled FROM data ORDER BY id",
    )
    .unwrap()
    .table;
    assert_eq!(result.col_count(), 2);
    assert_eq!(result.columns[0].name, "user_id");
    assert_eq!(result.columns[1].name, "scaled");
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(1)));
}

#[test]
fn test_aggregate_avg() {
    let table = sample_table();
    let result = run_query(&table, "SELECT AVG(score) AS avg_score FROM data")
        .unwrap()
        .table;
    assert_eq!(result.row_count(), 1);
    match result.get(0, 0) {
        Some(CellValue::Float(f)) => assert!((f - 8.25).abs() < 1e-6),
        other => panic!("expected float, got {other:?}"),
    }
}

#[test]
fn test_empty_query_errors() {
    let table = sample_table();
    assert!(run_query(&table, "   ").is_err());
}

#[test]
fn test_invalid_sql_errors() {
    let table = sample_table();
    assert!(run_query(&table, "SELECT * FROM nonexistent_table").is_err());
}

#[test]
fn test_query_against_empty_table() {
    let table = DataTable {
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
    };
    let result = run_query(&table, "SELECT COUNT(*) FROM data")
        .unwrap()
        .table;
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(0)));
}

#[test]
fn test_quoted_column_names_with_spaces() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "first name".into(),
            data_type: "Utf8".into(),
        }],
        rows: vec![vec![CellValue::String("Alice".into())]],
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
    };
    let result = run_query(&table, r#"SELECT "first name" FROM data"#)
        .unwrap()
        .table;
    assert_eq!(result.get(0, 0), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_large_table_query_completes_quickly() {
    // 100k rows × 4 cols. With prepared-statement-per-row this took
    // ~30s on a fast laptop; with Appender it should be sub-second.
    let row_count = 100_000;
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "label".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "flag".into(),
            data_type: "Boolean".into(),
        },
    ];
    let mut rows = Vec::with_capacity(row_count);
    for i in 0..row_count {
        rows.push(vec![
            CellValue::Int(i as i64),
            CellValue::String(format!("row{i}")),
            CellValue::Float(i as f64 * 0.5),
            CellValue::Bool(i % 2 == 0),
        ]);
    }
    let table = DataTable {
        columns,
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
    };
    let start = std::time::Instant::now();
    let result = run_query(&table, "SELECT COUNT(*) AS n FROM data")
        .unwrap()
        .table;
    let elapsed = start.elapsed();
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(row_count as i64)));
    assert!(
        elapsed.as_secs() < 10,
        "100k row query took {elapsed:?} — appender regression?"
    );
}

#[test]
fn test_null_values_passthrough() {
    let table = DataTable {
        columns: vec![ColumnInfo {
            name: "v".into(),
            data_type: "Utf8".into(),
        }],
        rows: vec![
            vec![CellValue::String("a".into())],
            vec![CellValue::Null],
            vec![CellValue::String("b".into())],
        ],
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
    };
    let result = run_query(&table, "SELECT COUNT(v) AS n FROM data")
        .unwrap()
        .table;
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(2)));
}

#[test]
fn test_update_mutates_base_table() {
    let table = sample_table();
    let outcome = run_query(&table, "UPDATE data SET name = 'ALICE' WHERE id = 1").unwrap();
    assert_eq!(outcome.kind, QueryKind::Mutation);
    assert_eq!(outcome.affected, Some(1));
    let mutated = outcome.table;
    assert_eq!(mutated.row_count(), 3);
    // Column types preserved from the original table.
    assert_eq!(mutated.columns[0].data_type, "Int64");
    let alice_row = (0..mutated.row_count())
        .find(|&r| mutated.get(r, 0) == Some(&CellValue::Int(1)))
        .expect("id=1 row should still exist");
    assert_eq!(
        mutated.get(alice_row, 1),
        Some(&CellValue::String("ALICE".into()))
    );
    assert!(mutated.structural_changes);
}

#[test]
fn test_delete_mutates_base_table() {
    let table = sample_table();
    let outcome = run_query(&table, "DELETE FROM data WHERE id = 2").unwrap();
    assert_eq!(outcome.kind, QueryKind::Mutation);
    assert_eq!(outcome.affected, Some(1));
    let mutated = outcome.table;
    assert_eq!(mutated.row_count(), 2);
    assert!(
        (0..mutated.row_count()).all(|r| mutated.get(r, 0) != Some(&CellValue::Int(2))),
        "id=2 should have been deleted",
    );
}

#[test]
fn test_insert_mutates_base_table() {
    let table = sample_table();
    let outcome = run_query(
        &table,
        "INSERT INTO data (id, name, score) VALUES (4, 'Dana', 6.5)",
    )
    .unwrap();
    assert_eq!(outcome.kind, QueryKind::Mutation);
    assert_eq!(outcome.affected, Some(1));
    let mutated = outcome.table;
    assert_eq!(mutated.row_count(), 4);
    let dana = (0..mutated.row_count())
        .find(|&r| mutated.get(r, 0) == Some(&CellValue::Int(4)))
        .expect("inserted id=4 row should be present");
    assert_eq!(
        mutated.get(dana, 1),
        Some(&CellValue::String("Dana".into()))
    );
}

/// DuckDB's SUMMARIZE backs the GUI's Analyse > Summary tab: one result row
/// per source column, with the column names in a `column_name` column.
#[test]
fn test_summarize_returns_one_row_per_column() {
    let table = sample_table();
    let result = run_query(&table, "SUMMARIZE data").expect("SUMMARIZE should work");
    assert_eq!(result.kind, QueryKind::Select);
    assert_eq!(
        result.table.row_count(),
        table.columns.len(),
        "one summary row per source column"
    );
    let name_col = result
        .table
        .columns
        .iter()
        .position(|c| c.name == "column_name")
        .expect("SUMMARIZE result has a column_name column");
    let mut names: Vec<String> = (0..result.table.row_count())
        .map(|r| {
            result
                .table
                .get(r, name_col)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .collect();
    names.sort();
    assert_eq!(names, vec!["id", "name", "score"]);
}
