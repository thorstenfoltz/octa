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
