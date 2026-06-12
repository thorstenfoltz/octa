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
