//! Unit tests for [`json_util`](json_util). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use serde_json::json;

#[test]
fn rename_top_level_key_preserves_order() {
    let mut v = json!({ "a": 1, "b": 2, "c": 3 });
    rename_object_key_at_path(&mut v, "", "b", "B").unwrap();
    let s = serde_json::to_string(&v).unwrap();
    assert_eq!(s, r#"{"a":1,"B":2,"c":3}"#);
}

#[test]
fn rename_nested_key() {
    let mut v = json!({ "outer": { "inner": 42 } });
    rename_object_key_at_path(&mut v, "outer", "inner", "INNER").unwrap();
    assert_eq!(v["outer"]["INNER"], 42);
}

#[test]
fn rename_collision_errors() {
    let mut v = json!({ "a": 1, "b": 2 });
    let err = rename_object_key_at_path(&mut v, "", "a", "b").unwrap_err();
    assert!(err.contains("already exists"), "got {err}");
}

#[test]
fn rename_array_path_errors_clearly() {
    let mut v = json!({ "arr": [1, 2, 3] });
    // The synthesized "key" of an array element is its index - not
    // renamable. The path navigator stops short and reports the type
    // mismatch.
    let err = rename_object_key_at_path(&mut v, "arr", "0", "first").unwrap_err();
    assert!(err.contains("not found") || err.contains("not an object"));
}

#[test]
fn add_object_key_appends() {
    let mut v = json!({ "a": 1 });
    add_object_key_at_path(&mut v, "", "b", json!(2)).unwrap();
    let s = serde_json::to_string(&v).unwrap();
    assert_eq!(s, r#"{"a":1,"b":2}"#);
}

#[test]
fn add_object_key_collision_errors() {
    let mut v = json!({ "a": 1 });
    let err = add_object_key_at_path(&mut v, "", "a", json!(2)).unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn add_object_key_empty_name_errors() {
    let mut v = json!({});
    let err = add_object_key_at_path(&mut v, "", "", json!(null)).unwrap_err();
    assert!(err.contains("empty"));
}
