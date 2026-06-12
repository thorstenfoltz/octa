//! Unit tests for [`gemini`](gemini). Split out of the source file; included
//! back via `#[path]` so it stays an inner `schema_tests` module with access to the
//! parent module's private items.

use super::*;

/// Recursively assert no key Gemini rejects survives as a *schema keyword*.
/// Property names are arbitrary (a tool may have a param called `title` or
/// `type`), so the values under `properties` are checked but their keys are
/// not treated as keywords.
fn assert_clean(v: &Value) {
    const BAD: &[&str] = &[
        "$schema",
        "$ref",
        "$defs",
        "definitions",
        "additionalProperties",
        "default",
        "title",
        "anyOf",
        "oneOf",
        "allOf",
    ];
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                if k == "properties" {
                    if let Value::Object(props) = val {
                        for pv in props.values() {
                            assert_clean(pv);
                        }
                    }
                    continue;
                }
                assert!(
                    !BAD.contains(&k.as_str()),
                    "leaked `{k}` into Gemini schema"
                );
                assert_clean(val);
            }
        }
        Value::Array(a) => {
            for val in a {
                assert_clean(val);
            }
        }
        _ => {}
    }
}

#[test]
fn nullable_option_collapses() {
    let s = json!({
        "type": "object",
        "properties": {
            "open_tab": { "anyOf": [ { "type": "string" }, { "type": "null" } ] }
        }
    });
    let out = to_gemini_schema(s);
    let ot = &out["properties"]["open_tab"];
    assert_eq!(ot["type"], "string");
    assert_eq!(ot["nullable"], true);
    assert_clean(&out);
}

#[test]
fn ref_is_inlined_and_defs_dropped() {
    let s = json!({
        "type": "object",
        "properties": { "spec": { "$ref": "#/$defs/Spec" } },
        "$defs": { "Spec": { "type": "object", "properties": { "n": { "type": "integer" } } } }
    });
    let out = to_gemini_schema(s);
    assert_eq!(out["properties"]["spec"]["type"], "object");
    assert_eq!(
        out["properties"]["spec"]["properties"]["n"]["type"],
        "integer"
    );
    assert_clean(&out);
}

#[test]
fn every_tool_schema_is_gemini_clean() {
    for def in crate::app::chat::tools::tool_defs() {
        let cleaned = to_gemini_schema(def.input_schema.clone());
        assert_eq!(cleaned["type"], "object", "tool {}", def.name);
        assert_clean(&cleaned);
    }
}
