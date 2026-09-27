//! Unit tests for [`text_ops`](text_ops). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn upper_lower_apply_helpers() {
    assert_eq!(CaseOp::Upper.apply("abc"), "ABC");
    assert_eq!(CaseOp::Lower.apply("XYZ"), "xyz");
}

#[test]
fn byte_range_basic() {
    let s = "hello";
    let r = char_range_to_byte_range(s, 1, 4);
    assert_eq!(r, 1..4);
    assert_eq!(&s[r], "ell");
}

#[test]
fn byte_range_unicode() {
    let s = "héllo";
    // chars: h, é, l, l, o
    let r = char_range_to_byte_range(s, 1, 4);
    // 'é' is 2 bytes in UTF-8.
    assert_eq!(&s[r], "éll");
}

#[test]
fn byte_range_clamped_at_end() {
    let s = "abc";
    let r = char_range_to_byte_range(s, 0, 3);
    assert_eq!(&s[r], "abc");
}

#[test]
fn a_typed_tab_becomes_spaces_and_the_cursor_follows() {
    // Cursor sits just after the tab egui inserted at index 1.
    let (text, cursor) = expand_tabs_at_cursor("a\tb", 2, 4).unwrap();
    assert_eq!(text, "a    b");
    assert_eq!(cursor, 5);
}

#[test]
fn tabs_the_file_already_had_are_left_alone() {
    // The cursor is at the end, nowhere near the file's own tabs: nothing to
    // do. Expanding here would rewrite the file just for being drawn.
    assert!(expand_tabs_at_cursor("\ta\tb", 4, 4).is_none());
    assert!(expand_tabs_at_cursor("\tindented", 9, 4).is_none());
}

#[test]
fn a_run_of_tabs_before_the_cursor_expands_together() {
    let (text, cursor) = expand_tabs_at_cursor("x\t\ty", 3, 2).unwrap();
    assert_eq!(text, "x    y");
    assert_eq!(cursor, 5);
}

#[test]
fn expansion_counts_characters_not_bytes() {
    // The leading character is multi-byte; a byte-based index would land
    // inside it and either miscount or panic.
    let (text, cursor) = expand_tabs_at_cursor("\u{e4}\tz", 2, 3).unwrap();
    assert_eq!(text, "\u{e4}   z");
    assert_eq!(cursor, 4);
}

#[test]
fn a_cursor_past_the_end_is_clamped() {
    assert!(expand_tabs_at_cursor("ab", 99, 4).is_none());
    let (text, cursor) = expand_tabs_at_cursor("a\t", 99, 2).unwrap();
    assert_eq!(text, "a  ");
    assert_eq!(cursor, 3);
}
