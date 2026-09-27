//! Unit tests for [`search`](search). Split out of the source file; included
//! back via `#[path]` so it stays an inner `option_tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn case_sensitive_plain() {
    let cs = RowMatcher::with_options("Foo", SearchMode::Plain, true, false);
    assert!(cs.matches("a Foo b"));
    assert!(!cs.matches("a foo b"));
    let ci = RowMatcher::with_options("Foo", SearchMode::Plain, false, false);
    assert!(ci.matches("a foo b"));
}

#[test]
fn whole_word_plain() {
    let ww = RowMatcher::with_options("cat", SearchMode::Plain, false, true);
    assert!(ww.matches("the cat sat"));
    assert!(!ww.matches("category"));
    assert!(!ww.matches("scatter"));
}

#[test]
fn whole_word_and_case_together() {
    let m = RowMatcher::with_options("ID", SearchMode::Plain, true, true);
    assert!(m.matches("the ID here"));
    assert!(!m.matches("the id here"));
    assert!(!m.matches("IDENT"));
}

#[test]
fn defaults_match_new_for_plain() {
    // both off == case-insensitive substring, same as `new`.
    let m = RowMatcher::with_options("bar", SearchMode::Plain, false, false);
    assert!(m.matches("BARimba"));
}

// --- The pattern handed to a SQL engine for a whole-file scan ----------------
//
// These pin the one property that matters: the scan asks the *same* question
// the search box asked. A scan that quietly downgraded a regex or a
// case-sensitive search to a plain substring match would report a count for a
// different question, in the same words.

use super::{RowMatcher, sql_regex_pattern};

/// Does the pattern the scan would send match the same text the in-memory
/// matcher does?
fn agree(query: &str, mode: SearchMode, case_sensitive: bool, whole_word: bool, text: &str) {
    let matcher = RowMatcher::with_options(query, mode, case_sensitive, whole_word);
    let pattern =
        sql_regex_pattern(query, mode, case_sensitive, whole_word).expect("valid pattern");
    let re = regex::Regex::new(&pattern).expect("pattern rebuilds");
    assert_eq!(
        matcher.matches(text),
        re.is_match(text),
        "{mode:?} case={case_sensitive} word={whole_word} query={query:?} text={text:?} \
         pattern={pattern:?}"
    );
}

#[test]
fn the_scan_pattern_agrees_with_the_in_memory_matcher() {
    for (query, mode) in [
        ("cat", SearchMode::Plain),
        ("c*t", SearchMode::Wildcard),
        ("^c.t$", SearchMode::Regex),
        ("a.b", SearchMode::Plain),
    ] {
        for case_sensitive in [false, true] {
            for whole_word in [false, true] {
                for text in ["cat", "CAT", "category", "a scatter", "c t", "a.b", "axb"] {
                    agree(query, mode, case_sensitive, whole_word, text);
                }
            }
        }
    }
}

#[test]
fn a_plain_query_is_escaped_not_interpreted() {
    // `a.b` in Plain mode must not match `axb`: the dot is a literal.
    let pattern = sql_regex_pattern("a.b", SearchMode::Plain, true, false).expect("pattern");
    let re = regex::Regex::new(&pattern).expect("rebuilds");
    assert!(re.is_match("a.b"));
    assert!(!re.is_match("axb"), "{pattern}");
}

#[test]
fn case_sensitivity_travels_with_the_pattern() {
    let insensitive = sql_regex_pattern("cat", SearchMode::Plain, false, false).expect("p");
    assert!(insensitive.starts_with("(?i)"), "{insensitive}");
    let sensitive = sql_regex_pattern("cat", SearchMode::Plain, true, false).expect("p");
    assert!(!sensitive.starts_with("(?i)"), "{sensitive}");
}

#[test]
fn an_unbuildable_pattern_yields_nothing_rather_than_broken_sql() {
    assert!(sql_regex_pattern("(unclosed", SearchMode::Regex, false, false).is_none());
}
