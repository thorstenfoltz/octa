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

#[test]
fn pretty_print_breaks_a_minified_object() {
    let out = pretty_print(r#"{"a":1,"b":[2,3]}"#);
    assert_eq!(out, "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}");
}

#[test]
fn pretty_print_keeps_punctuation_inside_strings() {
    // Braces, commas and escaped quotes in a string value must survive as
    // written - they are data, not structure.
    let src = r#"{"s":"a,b{c}[d] \"q\" \\"}"#;
    let out = pretty_print(src);
    assert_eq!(out, "{\n  \"s\": \"a,b{c}[d] \\\"q\\\" \\\\\"\n}");
}

#[test]
fn pretty_print_keeps_numbers_verbatim() {
    // The whole reason this does not go through serde_json::Value: that would
    // rewrite these three as 1.5, 100000.0 and a truncated integer.
    let out = pretty_print(r#"{"a":1.50,"b":1e5,"c":123456789012345678901234567890}"#);
    assert!(out.contains("1.50"), "{out}");
    assert!(out.contains("1e5"), "{out}");
    assert!(out.contains("123456789012345678901234567890"), "{out}");
}

#[test]
fn pretty_print_leaves_empty_containers_on_one_line() {
    assert_eq!(
        pretty_print(r#"{"a":{},"b":[]}"#),
        "{\n  \"a\": {},\n  \"b\": []\n}"
    );
}

#[test]
fn pretty_print_is_idempotent() {
    let src = r#"{"a":[{"b":1},{"c":[]}],"d":"x"}"#;
    let once = pretty_print(src);
    assert_eq!(pretty_print(&once), once);
}

#[test]
fn pretty_print_never_changes_what_the_json_means() {
    // The one check that matters for a re-indenter: the document still parses
    // to the same value. Covers a string holding structural punctuation, an
    // escaped tab, an empty container, an exponent and a null.
    let src = r#"{"a":[1,{"b":"x, y {z}","c":[]},true,null],"d":{"e":1.5e-3},"f":"tab\there"}"#;
    let out = pretty_print(src);
    let before: Value = serde_json::from_str(src).unwrap();
    let after: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(before, after, "formatted:\n{out}");
}
