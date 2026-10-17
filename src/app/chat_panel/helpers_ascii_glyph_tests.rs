use super::ascii_glyphs;

#[test]
fn maps_common_tofu_to_ascii() {
    assert_eq!(ascii_glyphs("a \u{2192} b"), "a -> b");
    assert_eq!(ascii_glyphs("x \u{2014} y \u{2026}"), "x - y ...");
    assert_eq!(ascii_glyphs("\u{201C}hi\u{201D}"), "\"hi\"");
    // Pure ASCII is borrowed unchanged; real script text is preserved.
    assert!(matches!(
        ascii_glyphs("plain"),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(ascii_glyphs("\u{4f60}\u{597d}"), "\u{4f60}\u{597d}");
}
