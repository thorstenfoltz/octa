//! Unit tests for [`ods_reader`](ods_reader). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn escape_xml_text_basic() {
    assert_eq!(escape_xml_text("a & b"), "a &amp; b");
    assert_eq!(escape_xml_text("<tag>"), "&lt;tag&gt;");
    assert_eq!(
        escape_xml_text("she said \"hi\""),
        "she said &quot;hi&quot;"
    );
}

#[test]
fn escape_xml_text_strips_control_chars() {
    assert_eq!(escape_xml_text("a\x01b\x02"), "ab");
    // Tab/LF/CR must survive
    assert_eq!(escape_xml_text("a\tb\nc\rd"), "a\tb\nc\rd");
}

#[test]
fn format_f64_renders_integer_form_when_exact() {
    assert_eq!(format_f64_for_attr(3.0), "3");
    assert_eq!(format_f64_for_attr(3.5), "3.5");
    assert_eq!(format_f64_for_attr(-2.0), "-2");
}

#[test]
fn format_f64_handles_non_finite() {
    assert_eq!(format_f64_for_attr(f64::NAN), "0");
    assert_eq!(format_f64_for_attr(f64::INFINITY), "0");
}
