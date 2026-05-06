use octa::data::CellValue;
use octa::formats::json_reader::*;
use serde_json::{Value, json};
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
fn test_flatten_objects_at_arbitrary_depth() {
    // Every leaf path must become its own column entry, regardless of how
    // many object levels separate it from the root.
    let val = json!({
        "a": {"b": {"c": {"d": {"e": 1}}}},
        "x": 2
    });
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    let keys: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"a.b.c.d.e"), "got keys: {:?}", keys);
    assert!(keys.contains(&"x"), "got keys: {:?}", keys);
}

#[test]
fn test_flatten_array_of_objects_at_depth() {
    // An object nested inside an array, nested inside an object, must
    // produce indexed leaf columns — not a single Nested cell.
    let val = json!({"data": {"items": [{"x": {"y": 1}}, {"x": {"y": 2}}]}});
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    let keys: Vec<&str> = out.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"data.items[0].x.y"), "got keys: {:?}", keys);
    assert!(keys.contains(&"data.items[1].x.y"), "got keys: {:?}", keys);
}

#[test]
fn test_flatten_array_of_primitives_stays_nested() {
    // ["a", "b"] inside an object stays as a single column whose value is
    // the JSON-encoded array — that's the rule the user picked.
    let val = json!({"tags": ["a", "b"]});
    let mut out = Vec::new();
    flatten_value("", &val, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "tags");
    match &out[0].1 {
        Value::Array(arr) => assert_eq!(arr.len(), 2),
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn test_json_to_table_deep_nested_with_arrays() {
    // Confirms the user-approved layout: nested objects flatten arbitrarily
    // deep; arrays remain JSON-string cells.
    let raw = std::fs::read_to_string("tests/fixtures/nested.json").unwrap();
    let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let table = json_to_table(val, &test_path(), "JSON").unwrap();
    assert_eq!(table.row_count(), 2);
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    for expected in [
        "user.name",
        "user.addr.city",
        "user.addr.zip",
        "user.addr.geo.lat",
        "user.addr.geo.lon",
        "tags",
    ] {
        assert!(
            names.contains(&expected),
            "missing column {}, got {:?}",
            expected,
            names
        );
    }
    let tags_idx = table.columns.iter().position(|c| c.name == "tags").unwrap();
    match table.get(0, tags_idx) {
        Some(CellValue::Nested(s)) => assert!(s.contains("\"a\"") && s.contains("\"b\"")),
        other => panic!("expected Nested array for tags, got {:?}", other),
    }
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
