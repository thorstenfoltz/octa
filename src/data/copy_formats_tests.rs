use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        ("name", "Utf8"),
        ("qty", "Int64"),
        ("name", "Utf8"),
        ("day", "Date32"),
    ]
    .iter()
    .map(|(n, ty)| ColumnInfo {
        name: (*n).into(),
        data_type: (*ty).into(),
    })
    .collect();
    t.rows = vec![
        vec![
            CellValue::String("O'Brien \"Bob\" <&>".into()),
            CellValue::Int(3),
            CellValue::String("x".into()),
            CellValue::Date("2026-10-31".into()),
        ],
        vec![
            CellValue::Null,
            CellValue::Float(2.5),
            CellValue::Null,
            CellValue::Null,
        ],
    ];
    t
}

fn all(t: &DataTable) -> Block<'_> {
    Block {
        table: t,
        cols: vec![0, 1, 2, 3],
        rows: vec![0, 1],
        only: None,
    }
}

#[test]
fn json_is_an_array_of_typed_objects_in_column_order() {
    let t = table();
    let v: serde_json::Value = serde_json::from_str(&to_json(&all(&t))).unwrap();
    assert_eq!(v[0]["name"], "O'Brien \"Bob\" <&>");
    assert_eq!(v[0]["qty"], 3);
    assert_eq!(v[0]["name_2"], "x");
    assert_eq!(v[0]["day"], "2026-10-31");
    assert!(v[1]["name"].is_null());
    assert_eq!(v[1]["qty"], 2.5);
    let keys: Vec<&String> = v[0].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["name", "qty", "name_2", "day"]);
}

#[test]
fn json_has_no_infinities_and_writes_durations_as_iso() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "f".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "d".into(),
            data_type: "Duration(Microsecond)".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Boolean".into(),
        },
    ];
    t.rows = vec![vec![
        CellValue::Float(f64::INFINITY),
        CellValue::Duration(90 * 60 * 1_000_000_000),
        CellValue::Bool(true),
    ]];
    let b = Block {
        table: &t,
        cols: vec![0, 1, 2],
        rows: vec![0],
        only: None,
    };
    let v: serde_json::Value = serde_json::from_str(&to_json(&b)).unwrap();
    assert!(v[0]["f"].is_null());
    assert!(v[0]["d"].as_str().unwrap().starts_with("PT"));
    assert_eq!(v[0]["b"], true);
}

#[test]
fn insert_quotes_text_and_identifiers_and_writes_null() {
    let t = table();
    let sql = to_insert(&all(&t), "my \"t\"");
    let lines: Vec<&str> = sql.lines().collect();
    assert_eq!(
        lines[0],
        r#"INSERT INTO "my ""t""" ("name", "qty", "name_2", "day") VALUES ('O''Brien "Bob" <&>', 3, 'x', '2026-10-31');"#
    );
    assert_eq!(
        lines[1],
        r#"INSERT INTO "my ""t""" ("name", "qty", "name_2", "day") VALUES (NULL, 2.5, NULL, NULL);"#
    );
}

#[test]
fn insert_writes_booleans_as_keywords() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "ok".into(),
        data_type: "Boolean".into(),
    }];
    t.rows = vec![vec![CellValue::Bool(true)], vec![CellValue::Bool(false)]];
    let b = Block {
        table: &t,
        cols: vec![0],
        rows: vec![0, 1],
        only: None,
    };
    assert!(
        to_insert(&b, "t").contains("VALUES (TRUE);\nINSERT INTO \"t\" (\"ok\") VALUES (FALSE);")
    );
}

#[test]
fn a_cell_outside_a_disjoint_selection_counts_as_empty() {
    let t = table();
    let only: std::collections::HashSet<(usize, usize)> = [(0, 1)].into_iter().collect();
    let b = Block {
        table: &t,
        cols: vec![0, 1],
        rows: vec![0],
        only: Some(&only),
    };
    let v: serde_json::Value = serde_json::from_str(&to_json(&b)).unwrap();
    assert!(v[0]["name"].is_null());
    assert_eq!(v[0]["qty"], 3);
}

#[test]
fn the_insert_table_name_comes_from_the_database_then_the_file() {
    assert_eq!(
        insert_table_name(Some("orders"), Some("/x/y.csv")),
        "orders"
    );
    assert_eq!(
        insert_table_name(None, Some("/x/2024 sales-q1.csv")),
        "t_2024_sales_q1"
    );
    assert_eq!(insert_table_name(None, Some("/x/---.csv")), "data");
    assert_eq!(insert_table_name(Some(""), None), "data");
}
