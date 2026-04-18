use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::sql::run_query;
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
    let result = run_query(&table, "SELECT * FROM data ORDER BY id").unwrap();
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.col_count(), 3);
    assert_eq!(result.get(0, 1), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_count_returns_scalar() {
    let table = sample_table();
    let result = run_query(&table, "SELECT COUNT(*) AS n FROM data").unwrap();
    assert_eq!(result.row_count(), 1);
    assert_eq!(result.col_count(), 1);
    assert_eq!(result.columns[0].name, "n");
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(3)));
}

#[test]
fn test_where_filter() {
    let table = sample_table();
    let result = run_query(&table, "SELECT name FROM data WHERE id > 1 ORDER BY id").unwrap();
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
    .unwrap();
    assert_eq!(result.col_count(), 2);
    assert_eq!(result.columns[0].name, "user_id");
    assert_eq!(result.columns[1].name, "scaled");
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(1)));
}

#[test]
fn test_aggregate_avg() {
    let table = sample_table();
    let result = run_query(&table, "SELECT AVG(score) AS avg_score FROM data").unwrap();
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
    let result = run_query(&table, "SELECT COUNT(*) FROM data").unwrap();
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
    let result = run_query(&table, r#"SELECT "first name" FROM data"#).unwrap();
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
    let result = run_query(&table, "SELECT COUNT(*) AS n FROM data").unwrap();
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
    let result = run_query(&table, "SELECT COUNT(v) AS n FROM data").unwrap();
    assert_eq!(result.get(0, 0), Some(&CellValue::Int(2)));
}
