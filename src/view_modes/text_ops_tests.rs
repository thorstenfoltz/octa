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
fn tabs_before_cursor_counts_ascii() {
    assert_eq!(tabs_before_cursor("\ta\tb", 4), 2);
    assert_eq!(tabs_before_cursor("\ta\tb", 2), 1);
    assert_eq!(tabs_before_cursor("\ta\tb", 0), 0);
}

#[test]
fn tabs_before_cursor_counts_chars_not_bytes() {
    // "ä" is 2 bytes but 1 char. The cursor sits after the tab at char 2, so
    // exactly one tab precedes it. Byte-slicing `s[..2]` would have cut the
    // string mid-"ä" and panicked; a byte-length comparison would have missed
    // the tab entirely.
    let s = "ä\tx";
    assert_eq!(tabs_before_cursor(s, 2), 1);
    assert_eq!(tabs_before_cursor(s, 1), 0);
}

#[test]
fn tabs_before_cursor_saturates_past_end() {
    // egui can hand back a cursor beyond the buffer after an external edit;
    // `take` clamps rather than panicking.
    assert_eq!(tabs_before_cursor("\t", 999), 1);
    assert_eq!(tabs_before_cursor("", 5), 0);
}
