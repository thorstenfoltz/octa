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

// --- Replace -----------------------------------------------------------------

#[test]
fn only_a_regex_search_expands_dollar_groups() {
    // Plain, plain with a toggle (routed through a regex) and wildcard all
    // insert the replacement as typed: "$5" is five dollars.
    for m in [
        RowMatcher::with_options("price", SearchMode::Plain, false, false),
        RowMatcher::with_options("price", SearchMode::Plain, true, false),
        RowMatcher::with_options("price", SearchMode::Plain, false, true),
        RowMatcher::with_options("pri*", SearchMode::Wildcard, false, false),
    ] {
        assert_eq!(m.replace_all("price", "$5"), "$5");
    }
    let re = RowMatcher::with_options(r"(\w+)@(\w+)", SearchMode::Regex, false, false);
    assert_eq!(re.replace_all("ann@corp", "$2:$1"), "corp:ann");
}

#[test]
fn replace_all_reaches_every_match_and_replace_only_the_first() {
    let m = RowMatcher::with_options("a", SearchMode::Plain, false, false);
    assert_eq!(m.replace_all("a, A, a", "b"), "b, b, b");
    assert_eq!(m.replace("a, A, a", "b"), "b, A, a");
}

#[test]
fn replace_range_touches_only_that_match() {
    let m = RowMatcher::with_options("x", SearchMode::Plain, false, false);
    let text = "x1 x2 x3";
    let ranges = m.find_ranges(text);
    assert_eq!(m.replace_range(text, ranges[1].clone(), "yy"), "x1 yy2 x3");
    let re = RowMatcher::with_options(r"x(\d)", SearchMode::Regex, false, false);
    let ranges = re.find_ranges(text);
    assert_eq!(
        re.replace_range(text, ranges[2].clone(), "<$1>"),
        "x1 x2 <3>"
    );
}

#[test]
fn replace_in_cells_counts_changed_cells_and_keeps_types() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("a-a".into()), CellValue::Int(10)],
        vec![CellValue::String("b".into()), CellValue::Int(11)],
    ];
    let m = RowMatcher::with_options("1", SearchMode::Plain, false, false);
    // Only the listed cells are touched: row 1's number is left alone.
    assert_eq!(replace_in_cells(&mut t, &[(0, 1)], &m, "2"), 1);
    assert_eq!(t.get(0, 1), Some(&CellValue::Int(20)));
    assert_eq!(t.get(1, 1), Some(&CellValue::Int(11)));
    let m = RowMatcher::with_options("a", SearchMode::Plain, false, false);
    assert_eq!(replace_in_cells(&mut t, &[(0, 0), (1, 0)], &m, "z"), 1);
    assert_eq!(t.get(0, 0), Some(&CellValue::String("z-z".into())));
}

#[test]
fn replace_next_takes_the_selected_match_then_moves_on() {
    let cells = [(0, 1), (2, 0), (5, 3)];
    let order = |c: (usize, usize)| c;
    // Selected cell is a match: replace it, select the one after.
    assert_eq!(
        replace_next_target(&cells, Some((2, 0)), order),
        Some(((2, 0), Some((5, 3))))
    );
    // Selected cell is not a match: the first match after it, wrapping.
    assert_eq!(
        replace_next_target(&cells, Some((3, 0)), order),
        Some(((5, 3), Some((0, 1))))
    );
    assert_eq!(
        replace_next_target(&cells, Some((9, 9)), order),
        Some(((0, 1), Some((2, 0))))
    );
    assert_eq!(replace_next_target(&cells, None, order).unwrap().0, (0, 1));
    // One match left: nothing to move on to.
    assert_eq!(
        replace_next_target(&[(1, 1)], None, order),
        Some(((1, 1), None))
    );
    assert_eq!(replace_next_target(&[], None, order), None);
}

#[test]
fn an_empty_search_replaces_nothing() {
    let m = RowMatcher::with_options("", SearchMode::Plain, false, false);
    assert_eq!(m.replace_all("abc", "x"), "abc");
}
