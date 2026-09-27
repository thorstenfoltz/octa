//! Unit tests for [`json_tree`](json_tree). Included back via `#[path]` so it
//! stays an inner `tests` module with access to the parent's private items.

use super::*;

/// Flatten a document and hand back one readable line per row, so a test can
/// say what the tree shows without reaching into egui.
fn rows_of(
    value: &serde_json::Value,
    expanded: &std::collections::HashSet<String>,
    nested: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<String> {
    let mut rows = Vec::new();
    flatten(
        value,
        "",
        NodePos {
            key: None,
            is_index: false,
            depth: 0,
            is_last: true,
        },
        expanded,
        nested,
        &mut rows,
    );
    rows.iter()
        .map(|r| {
            let key = r.key.clone().unwrap_or_default();
            match &r.kind {
                JsonRowKind::Open { is_object, .. } => {
                    format!("{key}: {}", if *is_object { "{" } else { "[" })
                }
                JsonRowKind::Close { is_object } => {
                    (if *is_object { "}" } else { "]" }).to_string()
                }
                JsonRowKind::Leaf { value } => format!("{key}: {value}"),
            }
        })
        .collect()
}

/// A column of stringified JSON is the shape this unfold exists for: the
/// value is one long string, and what the user wants out of it is keys.
#[test]
fn a_string_holding_json_unfolds_into_keyed_rows() {
    let doc = serde_json::json!({
        "reports": r#"[{"id":"25aa493d","name":"Lot A"}]"#,
    });
    let expanded = std::collections::HashSet::from(["".to_string()]);

    // Folded: one leaf, the whole document on a single line.
    let folded = rows_of(&doc, &expanded, &Default::default());
    assert_eq!(folded.len(), 3, "open, one leaf, close: {folded:?}");

    // Unfolded, the way the toggle does it: parse, then open every level.
    let inner = parse_nested_json(doc["reports"].as_str().expect("a string"));
    let prefix = format!("reports{NESTED_MARK}");
    let mut expanded = expanded;
    expanded.extend(octa::data::json_util::collect_json_paths_under(
        &inner, &prefix,
    ));
    let nested = std::collections::HashMap::from([("reports".to_string(), inner)]);

    let unfolded = rows_of(&doc, &expanded, &nested);
    assert!(
        unfolded.iter().any(|r| r == "id: \"25aa493d\""),
        "the nested keys are rows of their own: {unfolded:?}"
    );
    assert!(
        unfolded.iter().any(|r| r == "name: \"Lot A\""),
        "{unfolded:?}"
    );
}

/// Those rows are display only: they carry a path the file does not have, and
/// every edit path is gated on it.
#[test]
fn unfolded_rows_are_not_part_of_the_document() {
    assert!(is_nested_path(&format!("reports{NESTED_MARK}[0].id")));
    assert!(!is_nested_path("reports[0].id"));
}

/// A value that only looked like JSON says so, instead of unfolding to
/// nothing at all.
#[test]
fn a_string_that_only_looked_like_json_says_so() {
    assert!(looks_like_nested_json("[{\"id\":1}]"));
    assert!(!looks_like_nested_json("Lot A"));
    let v = parse_nested_json("[{oops]");
    assert!(v.as_str().is_some_and(|s| !s.is_empty()), "{v:?}");
}

/// The shape this file actually has: JSON encoded as a string, whose records
/// carry another JSON string, which carries another. One click has to open
/// all of it - unfolding one link left "the most part just one line".
#[test]
fn a_chain_of_json_inside_json_unfolds_in_one_click() {
    let innermost = r#"{"lot":"Lot 7","ok":true}"#;
    let middle = format!(
        r#"[{{"id":"25aa493d","detail":{}}}]"#,
        serde_json::Value::String(innermost.to_string())
    );
    let doc = serde_json::json!({ "reports": middle });

    let mut docs = std::collections::HashMap::new();
    let mut expanded = std::collections::HashSet::from(["".to_string()]);
    unfold_nested_json(
        doc["reports"].as_str().expect("a string"),
        "reports",
        &mut docs,
        &mut expanded,
        0,
    );
    assert_eq!(docs.len(), 2, "both links parsed: {:?}", docs.keys());

    let rows = rows_of(&doc, &expanded, &docs);
    assert!(
        rows.iter().any(|r| r == "id: \"25aa493d\""),
        "first link: {rows:?}"
    );
    assert!(
        rows.iter().any(|r| r == "lot: \"Lot 7\""),
        "second link, the one that used to stay a single line: {rows:?}"
    );
    assert!(rows.iter().any(|r| r == "ok: true"), "{rows:?}");
}

/// Folding the outer leaf takes the whole chain with it, rather than leaving
/// the inner documents parsed and half the tree still open.
#[test]
fn folding_the_outer_leaf_drops_the_whole_chain() {
    let doc = serde_json::json!({
        "reports": r#"[{"detail":"{\"lot\":\"Lot 7\"}"}]"#,
    });
    let mut docs = std::collections::HashMap::new();
    let mut expanded = std::collections::HashSet::new();
    unfold_nested_json(
        doc["reports"].as_str().expect("a string"),
        "reports",
        &mut docs,
        &mut expanded,
        0,
    );
    assert_eq!(docs.len(), 2);

    let prefix = format!("reports{NESTED_MARK}");
    docs.remove("reports");
    docs.retain(|p, _| !p.starts_with(&prefix));
    expanded.retain(|p| !p.starts_with(&prefix));
    assert!(docs.is_empty(), "{:?}", docs.keys());
    assert!(expanded.is_empty(), "{expanded:?}");
}

/// Both limits hold, so a pathological blob cannot hang the frame.
#[test]
fn the_chain_stops_at_its_budget() {
    // Each link wraps the one below, so depth is the only thing that ends it.
    let mut raw = r#"{"end":1}"#.to_string();
    for _ in 0..20 {
        raw = format!(r#"{{"next":{}}}"#, serde_json::Value::String(raw));
    }
    let mut docs = std::collections::HashMap::new();
    let mut expanded = std::collections::HashSet::new();
    unfold_nested_json(&raw, "blob", &mut docs, &mut expanded, 0);
    assert_eq!(docs.len(), NESTED_MAX_DEPTH + 1, "{:?}", docs.len());
}
