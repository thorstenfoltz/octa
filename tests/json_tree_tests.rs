use octa::data::json_util::*;
use serde_json::json;

// --- collect_json_paths ---

#[test]
fn collect_paths_flat_object_all() {
    let v = json!({"a": 1, "b": "hello"});
    let paths = collect_json_paths(&v, None);
    // Root object is the only container
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(""));
}

#[test]
fn collect_paths_nested_object_all() {
    let v = json!({"a": {"b": {"c": 1}}, "d": 2});
    let paths = collect_json_paths(&v, None);
    assert!(paths.contains(""));
    assert!(paths.contains("a"));
    assert!(paths.contains("a.b"));
    assert!(!paths.contains("a.b.c")); // leaf, not a container
    assert_eq!(paths.len(), 3);
}

#[test]
fn collect_paths_array_all() {
    let v = json!([1, [2, 3], {"x": 4}]);
    let paths = collect_json_paths(&v, None);
    assert!(paths.contains(""));     // root array
    assert!(paths.contains("[1]"));  // nested array
    assert!(paths.contains("[2]"));  // nested object
    assert_eq!(paths.len(), 3);
}

#[test]
fn collect_paths_depth_0() {
    let v = json!({"a": {"b": 1}, "c": [1, 2]});
    let paths = collect_json_paths(&v, Some(0));
    // depth 0 = root only
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(""));
}

#[test]
fn collect_paths_depth_1() {
    let v = json!({"a": {"b": 1}, "c": [1, 2]});
    let paths = collect_json_paths(&v, Some(1));
    assert!(paths.contains(""));
    assert!(paths.contains("a"));
    assert!(paths.contains("c"));
    assert_eq!(paths.len(), 3);
}

#[test]
fn collect_paths_depth_2() {
    let v = json!({"a": {"b": {"d": 1}}, "c": [{"e": 2}]});
    let paths = collect_json_paths(&v, Some(2));
    assert!(paths.contains(""));
    assert!(paths.contains("a"));
    assert!(paths.contains("a.b"));
    assert!(paths.contains("c"));
    assert!(paths.contains("c[0]"));
    assert_eq!(paths.len(), 5);
}

#[test]
fn collect_paths_empty_object() {
    let v = json!({});
    let paths = collect_json_paths(&v, None);
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(""));
}

#[test]
fn collect_paths_empty_array() {
    let v = json!([]);
    let paths = collect_json_paths(&v, None);
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(""));
}

#[test]
fn collect_paths_scalar_root() {
    let v = json!(42);
    let paths = collect_json_paths(&v, None);
    assert!(paths.is_empty()); // scalars are not containers
}

#[test]
fn collect_paths_mixed_nesting() {
    let v = json!({"users": [{"name": "Alice", "tags": ["a", "b"]}, {"name": "Bob"}]});
    let paths = collect_json_paths(&v, None);
    assert!(paths.contains(""));
    assert!(paths.contains("users"));
    assert!(paths.contains("users[0]"));
    assert!(paths.contains("users[0].tags"));
    assert!(paths.contains("users[1]"));
    assert_eq!(paths.len(), 5);
}

// --- set_json_value_at_path ---

#[test]
fn set_value_simple_key() {
    let mut v = json!({"a": 1});
    set_json_value_at_path(&mut v, "a", json!(42)).unwrap();
    assert_eq!(v["a"], json!(42));
}

#[test]
fn set_value_nested_dot_path() {
    let mut v = json!({"a": {"b": {"c": 1}}});
    set_json_value_at_path(&mut v, "a.b.c", json!("hello")).unwrap();
    assert_eq!(v["a"]["b"]["c"], json!("hello"));
}

#[test]
fn set_value_array_index() {
    let mut v = json!({"items": [10, 20, 30]});
    set_json_value_at_path(&mut v, "items[1]", json!(99)).unwrap();
    assert_eq!(v["items"][1], json!(99));
}

#[test]
fn set_value_mixed_path() {
    let mut v = json!({"data": [{"name": "Alice"}, {"name": "Bob"}]});
    set_json_value_at_path(&mut v, "data[1].name", json!("Charlie")).unwrap();
    assert_eq!(v["data"][1]["name"], json!("Charlie"));
}

#[test]
fn set_value_root_replace() {
    let mut v = json!(42);
    set_json_value_at_path(&mut v, "", json!("replaced")).unwrap();
    assert_eq!(v, json!("replaced"));
}

#[test]
fn set_value_invalid_key() {
    let mut v = json!({"a": 1});
    let result = set_json_value_at_path(&mut v, "nonexistent.deep", json!(1));
    assert!(result.is_err());
}

#[test]
fn set_value_array_out_of_bounds() {
    let mut v = json!({"arr": [1, 2]});
    let result = set_json_value_at_path(&mut v, "arr[5]", json!(99));
    assert!(result.is_err());
}

#[test]
fn set_value_type_mismatch_not_object() {
    let mut v = json!({"a": 42});
    let result = set_json_value_at_path(&mut v, "a.b", json!(1));
    assert!(result.is_err());
}

#[test]
fn set_value_type_mismatch_not_array() {
    let mut v = json!({"a": "text"});
    let result = set_json_value_at_path(&mut v, "a[0]", json!(1));
    assert!(result.is_err());
}

// --- max_json_depth ---

#[test]
fn max_depth_scalar() {
    assert_eq!(max_json_depth(&json!(42)), 0);
    assert_eq!(max_json_depth(&json!("hello")), 0);
    assert_eq!(max_json_depth(&json!(null)), 0);
}

#[test]
fn max_depth_flat_object() {
    assert_eq!(max_json_depth(&json!({"a": 1, "b": 2})), 1);
}

#[test]
fn max_depth_flat_array() {
    assert_eq!(max_json_depth(&json!([1, 2, 3])), 1);
}

#[test]
fn max_depth_nested() {
    assert_eq!(max_json_depth(&json!({"a": {"b": {"c": 1}}})), 3);
}

#[test]
fn max_depth_mixed() {
    let v = json!({"users": [{"name": "Alice", "tags": ["a", "b"]}, {"name": "Bob"}]});
    assert_eq!(max_json_depth(&v), 4); // root -> users -> [0] -> tags -> items
}

#[test]
fn max_depth_empty_containers() {
    assert_eq!(max_json_depth(&json!({})), 0);
    assert_eq!(max_json_depth(&json!([])), 0);
}

// --- parse_json_edit ---

#[test]
fn parse_null() {
    assert_eq!(parse_json_edit("null"), json!(null));
}

#[test]
fn parse_bool_true() {
    assert_eq!(parse_json_edit("true"), json!(true));
}

#[test]
fn parse_bool_false() {
    assert_eq!(parse_json_edit("false"), json!(false));
}

#[test]
fn parse_integer() {
    assert_eq!(parse_json_edit("42"), json!(42));
}

#[test]
fn parse_negative_integer() {
    assert_eq!(parse_json_edit("-7"), json!(-7));
}

#[test]
fn parse_float() {
    assert_eq!(parse_json_edit("3.14"), json!(3.14));
}

#[test]
fn parse_string_fallback() {
    assert_eq!(parse_json_edit("hello world"), json!("hello world"));
}

#[test]
fn parse_empty_string() {
    assert_eq!(parse_json_edit(""), json!(""));
}

#[test]
fn parse_whitespace_trimmed() {
    assert_eq!(parse_json_edit("  true  "), json!(true));
    assert_eq!(parse_json_edit("  42  "), json!(42));
}
