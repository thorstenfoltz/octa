use octo::data::CellValue;
use octo::formats::json_reader::*;
use serde_json::json;
use std::path::PathBuf;

fn test_path() -> PathBuf {
    PathBuf::from("/tmp/test.json")
}

// --- json_value_to_cell ---

#[test]
fn test_json_null() {
    assert_eq!(json_value_to_cell(&json!(null)), CellValue::Null);
}

#[test]
fn test_json_bool() {
    assert_eq!(json_value_to_cell(&json!(true)), CellValue::Bool(true));
    assert_eq!(json_value_to_cell(&json!(false)), CellValue::Bool(false));
}

#[test]
fn test_json_int() {
    assert_eq!(json_value_to_cell(&json!(42)), CellValue::Int(42));
    assert_eq!(json_value_to_cell(&json!(-1)), CellValue::Int(-1));
}

#[test]
fn test_json_float() {
    assert_eq!(json_value_to_cell(&json!(3.14)), CellValue::Float(3.14));
}

#[test]
fn test_json_string() {
    assert_eq!(
        json_value_to_cell(&json!("hello")),
        CellValue::String("hello".into())
    );
}

#[test]
fn test_json_array_becomes_nested() {
    let val = json!([1, 2, 3]);
    match json_value_to_cell(&val) {
        CellValue::Nested(s) => assert_eq!(s, "[1,2,3]"),
        other => panic!("Expected Nested, got {:?}", other),
    }
}

#[test]
fn test_json_object_becomes_nested() {
    let val = json!({"a": 1});
    match json_value_to_cell(&val) {
        CellValue::Nested(s) => assert!(s.contains("\"a\":1") || s.contains("\"a\": 1")),
        other => panic!("Expected Nested, got {:?}", other),
    }
}

// --- flatten_value ---

#[test]
fn test_flatten_simple_object() {
    let val = json!({"name": "Alice", "age": 30});
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|(k, _)| k == "name"));
    assert!(out.iter().any(|(k, _)| k == "age"));
}

#[test]
fn test_flatten_nested_object() {
    let val = json!({"user": {"name": "Alice", "address": {"city": "Berlin"}}});
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    assert!(out.iter().any(|(k, _)| k == "user.name"));
    assert!(out.iter().any(|(k, _)| k == "user.address.city"));
}

#[test]
fn test_flatten_scalar() {
    let val = json!(42);
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "value");
}

// --- json_to_table ---

#[test]
fn test_json_array_of_objects() {
    let val = json!([
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"}
    ]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(table.get(1, 1), Some(&CellValue::String("Bob".into())));
}

#[test]
fn test_json_object_with_array_field() {
    let val = json!({
        "data": [
            {"x": 1},
            {"x": 2}
        ]
    });
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));
}

#[test]
fn test_json_empty_array() {
    let val = json!([]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.row_count(), 0);
    assert_eq!(table.col_count(), 0);
}

#[test]
fn test_json_scalar_value() {
    let val = json!(42);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(42)));
}

#[test]
fn test_json_sparse_objects() {
    let val = json!([
        {"a": 1},
        {"b": 2},
        {"a": 3, "b": 4}
    ]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(table.get(0, 1), Some(&CellValue::Null));
    assert_eq!(table.get(1, 0), Some(&CellValue::Null));
    assert_eq!(table.get(1, 1), Some(&CellValue::Int(2)));
}

// --- column type refinement ---

#[test]
fn test_json_int_column_type() {
    let val = json!([{"v": 1}, {"v": 2}]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.columns[0].data_type, "Int64");
}

#[test]
fn test_json_float_column_type() {
    let val = json!([{"v": 1.5}, {"v": 2.5}]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.columns[0].data_type, "Float64");
}

#[test]
fn test_json_bool_column_type() {
    let val = json!([{"v": true}, {"v": false}]);
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.columns[0].data_type, "Boolean");
}
