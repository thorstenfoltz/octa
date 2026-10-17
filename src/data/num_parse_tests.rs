use super::*;

#[test]
fn european_decimal_is_certain() {
    assert_eq!(
        classify_value("3,14"),
        ValueStyle::Certain(NumberStyle::European)
    );
    assert_eq!(
        classify_value("1.234,56"),
        ValueStyle::Certain(NumberStyle::European)
    );
    assert_eq!(
        classify_value("1.234.567"),
        ValueStyle::Certain(NumberStyle::European)
    );
}

#[test]
fn english_decimal_is_certain() {
    assert_eq!(
        classify_value("3.14"),
        ValueStyle::Certain(NumberStyle::English)
    );
    assert_eq!(
        classify_value("1,234.56"),
        ValueStyle::Certain(NumberStyle::English)
    );
    assert_eq!(
        classify_value("1,234,567"),
        ValueStyle::Certain(NumberStyle::English)
    );
}

#[test]
fn lone_separator_with_three_digits_is_ambiguous() {
    assert_eq!(classify_value("1,234"), ValueStyle::Ambiguous);
    assert_eq!(classify_value("1.234"), ValueStyle::Ambiguous);
}

#[test]
fn plain_and_non_numeric_values() {
    assert_eq!(classify_value("1234"), ValueStyle::Ambiguous);
    assert_eq!(classify_value("-42"), ValueStyle::Ambiguous);
    assert_eq!(classify_value("abc"), ValueStyle::NotNumeric);
    assert_eq!(classify_value(""), ValueStyle::NotNumeric);
    assert_eq!(classify_value("2024-01-31"), ValueStyle::NotNumeric);
}

/// The guard that keeps German dates out of the number pass: 31.12.2024 has
/// groups of 2 and 4 digits, which is not valid grouping under either style.
#[test]
fn dotted_date_is_not_a_number() {
    assert_eq!(classify_value("31.12.2024"), ValueStyle::NotNumeric);
    assert_eq!(classify_value("1.12.2024"), ValueStyle::NotNumeric);
}

#[test]
fn parses_both_styles() {
    assert_eq!(
        parse_number("1.234,56", NumberStyle::European),
        Some(1234.56)
    );
    // 3,25 rather than 3,14: clippy reads the latter as an approximation
    // of PI and denies the literal.
    assert_eq!(parse_number("3,25", NumberStyle::European), Some(3.25));
    assert_eq!(
        parse_number("1,234.56", NumberStyle::English),
        Some(1234.56)
    );
    assert_eq!(parse_number("1234", NumberStyle::European), Some(1234.0));
    assert_eq!(
        parse_number("-1.234,5", NumberStyle::European),
        Some(-1234.5)
    );
    assert_eq!(parse_number("abc", NumberStyle::English), None);
}

/// French exports separate groups with a space, including U+00A0 and U+2009.
#[test]
fn space_grouping_is_european() {
    assert_eq!(
        classify_value("1 234,56"),
        ValueStyle::Certain(NumberStyle::European)
    );
    assert_eq!(
        parse_number("1\u{00a0}234,56", NumberStyle::European),
        Some(1234.56)
    );
    assert_eq!(
        parse_number("1\u{2009}234,56", NumberStyle::European),
        Some(1234.56)
    );
}

#[test]
fn relaxed_parse_prefers_plain_then_european() {
    // Unchanged from today's behaviour: a bare English decimal still wins.
    assert_eq!(parse_number_relaxed("1.234"), Some(1.234));
    assert_eq!(parse_number_relaxed("3,25"), Some(3.25));
    assert_eq!(parse_number_relaxed("1.234,56"), Some(1234.56));
    assert_eq!(parse_number_relaxed("nope"), None);
}

#[test]
fn snapshot_promotion_reports_promoted_and_ambiguous() {
    use crate::data::ColumnInfo;
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "amount".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "code".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::String("1.234,56".into()),
            CellValue::String("1,234".into()),
        ],
        vec![
            CellValue::String("9,90".into()),
            CellValue::String("2,345".into()),
        ],
    ];

    let (promoted, ambiguous) = promote_columns_with_snapshot(&mut t);

    assert_eq!(promoted.len(), 1);
    assert_eq!(promoted[0].0, 0);
    assert_eq!(promoted[0].1, NumberStyle::European);
    assert_eq!(promoted[0].2[0], Some("1.234,56".to_string()));
    assert_eq!(t.get(0, 0), Some(&CellValue::Float(1234.56)));
    assert_eq!(ambiguous, vec![1]);
    // The ambiguous column is untouched until the user answers.
    assert_eq!(t.columns[1].data_type, "Utf8");
}

#[test]
fn column_inference_resolves_by_majority_of_certainty() {
    let european = [Some("1.234,56"), Some("1,20"), Some("1.234"), None];
    assert_eq!(
        infer_column(&european),
        NumberOutcome::Promote(NumberStyle::European)
    );

    let english = [Some("1,234.56"), Some("3.14"), None];
    assert_eq!(
        infer_column(&english),
        NumberOutcome::Promote(NumberStyle::English)
    );

    let ambiguous = [Some("1,234"), Some("2,345"), None];
    assert_eq!(infer_column(&ambiguous), NumberOutcome::Ambiguous);

    // A column with both certainties is a mixed mess: leave it alone.
    let conflicting = [Some("1.234,56"), Some("1,234.56")];
    assert_eq!(infer_column(&conflicting), NumberOutcome::Skip);

    // Nothing numeric, or already plain integers only: nothing to do.
    assert_eq!(
        infer_column(&[Some("abc"), Some("def")]),
        NumberOutcome::Skip
    );
    assert_eq!(infer_column(&[Some("12"), Some("34")]), NumberOutcome::Skip);
    assert_eq!(infer_column(&[]), NumberOutcome::Skip);

    // One stray word disqualifies the column: a real number column has no prose.
    assert_eq!(
        infer_column(&[Some("1.234,56"), Some("n/a")]),
        NumberOutcome::Skip
    );
}
