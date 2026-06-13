//! Unit tests for [`raw_text`](raw_text). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn split_plain_csv() {
    let r = split_delimited_line("a,b,c", ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r, vec!["a", "b", "c"]);
}

#[test]
fn split_quoted_comma_inside() {
    let r = split_delimited_line(
        r#""Smith, A","note","x""#,
        ',',
        RawCsvQuote::Double,
        RawCsvEscape::Doubled,
    );
    assert_eq!(r, vec!["Smith, A", "note", "x"]);
}

#[test]
fn split_doubled_quote_escape() {
    // "a""b" -> a"b
    let r = split_delimited_line(
        r#""a""b","c""#,
        ',',
        RawCsvQuote::Double,
        RawCsvEscape::Doubled,
    );
    assert_eq!(r, vec![r#"a"b"#, "c"]);
}

#[test]
fn split_backslash_escape() {
    let r = split_delimited_line(
        r#""a\"b","c""#,
        ',',
        RawCsvQuote::Double,
        RawCsvEscape::Backslash,
    );
    assert_eq!(r, vec![r#"a"b"#, "c"]);
}

#[test]
fn split_single_quotes() {
    let r = split_delimited_line(
        "'Smith, A',note",
        ',',
        RawCsvQuote::Single,
        RawCsvEscape::None,
    );
    assert_eq!(r, vec!["Smith, A", "note"]);
}

#[test]
fn split_either_quote() {
    let r = split_delimited_line(
        r#""a, b",'c, d',e"#,
        ',',
        RawCsvQuote::Both,
        RawCsvEscape::None,
    );
    assert_eq!(r, vec!["a, b", "c, d", "e"]);
}

#[test]
fn split_none_mode_treats_quotes_as_literal() {
    let r = split_delimited_line(r#""a,b",c"#, ',', RawCsvQuote::None, RawCsvEscape::None);
    assert_eq!(r, vec![r#""a"#, r#"b""#, "c"]);
}

#[test]
fn ranges_plain_csv() {
    let line = "a,b,c";
    let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r.len(), 3);
    assert_eq!(&line[r[0].clone()], "a");
    assert_eq!(&line[r[1].clone()], "b");
    assert_eq!(&line[r[2].clone()], "c");
}

#[test]
fn ranges_quoted_field_with_internal_delim_is_one_column() {
    // The whole `"1,2,3,4,5"` must come back as one column range - that's
    // the bug the colored layouter was hitting before this fix.
    let line = r#""1,2,3,4,5",foo"#;
    let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r.len(), 2);
    assert_eq!(&line[r[0].clone()], r#""1,2,3,4,5""#);
    assert_eq!(&line[r[1].clone()], "foo");
}

#[test]
fn ranges_handle_doubled_quote_escape() {
    let line = r#""a""b",c"#;
    let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r.len(), 2);
    assert_eq!(&line[r[0].clone()], r#""a""b""#);
    assert_eq!(&line[r[1].clone()], "c");
}

#[test]
fn format_preserves_quotes_around_embedded_delimiter() {
    // After alignment the cell `"1,2,3,4,5"` keeps its quotes so the
    // tokenizer can group it as one column when re-rendered.
    let formatted = format_delimited_text(
        r#""1,2,3,4,5",foo"#,
        ',',
        RawCsvQuote::Double,
        RawCsvEscape::Doubled,
    );
    let r =
        split_delimited_line_ranges(&formatted, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r.len(), 2);
    assert!(formatted[r[0].clone()].starts_with('"'));
    assert!(formatted[r[0].clone()].contains("1,2,3,4,5"));
}

#[test]
fn ranges_round_trip_after_alignment_two_quoted_fields() {
    // After alignment the join inserts a space after each delimiter, so
    // the second quoted field starts with whitespace before its quote.
    // The tokenizer must still group it into one range.
    let raw = r#""1,2","foo,bar""#;
    let formatted = format_delimited_text(raw, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    let r =
        split_delimited_line_ranges(&formatted, ',', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(
        r.len(),
        2,
        "formatted line {formatted:?} should still tokenize as 2 columns"
    );
    assert!(formatted[r[0].clone()].contains("1,2"));
    assert!(formatted[r[1].clone()].contains("foo,bar"));
}

#[test]
fn ranges_tsv_with_tab_delimiter_does_not_eat_tabs() {
    // Tab is the delimiter here, so the leading-whitespace skip MUST NOT
    // consume tab characters - they are separators, not padding.
    let line = "a\t\"b\tc\"\td";
    let r = split_delimited_line_ranges(line, '\t', RawCsvQuote::Double, RawCsvEscape::Doubled);
    assert_eq!(r.len(), 3);
    assert_eq!(&line[r[0].clone()], "a");
    assert_eq!(&line[r[1].clone()], "\"b\tc\"");
    assert_eq!(&line[r[2].clone()], "d");
}

#[test]
fn ranges_backslash_escape_at_field_start() {
    // A literal backslash at field start shouldn't be confused with the
    // quote-mode escape handling (which only applies inside quotes).
    let line = r#"\"a,b"#;
    let r = split_delimited_line_ranges(line, ',', RawCsvQuote::Double, RawCsvEscape::Backslash);
    // Field starts with `\` (not a quote), so no quote mode entered;
    // delimiter still splits.
    assert_eq!(r.len(), 2);
    assert_eq!(&line[r[0].clone()], r#"\"a"#);
    assert_eq!(&line[r[1].clone()], "b");
}
