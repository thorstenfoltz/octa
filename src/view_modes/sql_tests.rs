//! Unit tests for [`sql`](sql). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn prefix_picks_up_word_before_cursor() {
    let s = "SELECT na";
    let (start, pfx) = current_prefix_at(s, s.len());
    assert_eq!(pfx, "na");
    assert_eq!(start, 7);
}

#[test]
fn prefix_is_empty_after_whitespace() {
    let s = "SELECT ";
    let (start, pfx) = current_prefix_at(s, s.len());
    assert_eq!(pfx, "");
    assert_eq!(start, s.len());
}

#[test]
fn suggestions_match_columns_and_keywords() {
    let cols = vec!["name".to_string(), "age".to_string()];
    let out = collect_suggestions("n", &cols, 8);
    assert!(out.contains(&"name".to_string()));
    assert!(out.contains(&"NOT".to_string()));
}

#[test]
fn suggestions_respect_limit() {
    let cols: Vec<String> = (0..20).map(|i| format!("col_{i}")).collect();
    let out = collect_suggestions("col", &cols, 5);
    assert_eq!(out.len(), 5);
}

#[test]
fn empty_prefix_yields_no_suggestions() {
    let cols = vec!["name".to_string()];
    let out = collect_suggestions("", &cols, 8);
    assert!(out.is_empty());
}
