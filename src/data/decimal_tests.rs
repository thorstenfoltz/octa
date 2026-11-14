use super::*;
use crate::data::{ColumnInfo, DataTable};

#[test]
fn a_type_name_says_its_precision_and_scale() {
    assert_eq!(decimal_parts("Decimal128(18, 2)"), Some((18, 2)));
    assert_eq!(decimal_parts("decimal128(38,0)"), Some((38, 0)));
    for no in [
        "Float64",
        "Decimal256(38, 10)",
        "Decimal128(39, 2)",
        "Decimal128(10, 11)",
        "Decimal128(10, -2)",
        "Decimal128(0, 0)",
        "Decimal128(abc)",
    ] {
        assert_eq!(decimal_parts(no), None, "{no}");
    }
}

#[test]
fn the_type_crosses_to_duckdb_and_back() {
    assert_eq!(
        from_duckdb_type("DECIMAL(18,2)").as_deref(),
        Some("Decimal128(18, 2)")
    );
    assert_eq!(
        duckdb_type("Decimal128(18, 2)").as_deref(),
        Some("DECIMAL(18,2)")
    );
    assert_eq!(from_duckdb_type("DOUBLE"), None);
    assert_eq!(duckdb_type("Float64"), None);
}

#[test]
fn every_digit_is_kept_in_text() {
    for (raw, scale, text) in [
        (1234, 2, "12.34"),
        (150, 2, "1.50"),
        (5, 2, "0.05"),
        (-5, 2, "-0.05"),
        (0, 2, "0.00"),
        (7, 0, "7"),
        (-7, 0, "-7"),
        (
            12_345_678_901_234_567_890_123_456_789_012_345_678,
            2,
            "123456789012345678901234567890123456.78",
        ),
    ] {
        assert_eq!(decimal_text(raw, scale), text);
    }
}

#[test]
fn text_parses_exactly() {
    assert_eq!(parse_decimal("12.34", 18, 2), Some(1234));
    assert_eq!(
        parse_decimal("1.5", 18, 2),
        Some(150),
        "padded to the scale"
    );
    assert_eq!(parse_decimal("7", 18, 2), Some(700));
    assert_eq!(parse_decimal(" -0.05 ", 18, 2), Some(-5));
    assert_eq!(parse_decimal("+1.00", 18, 2), Some(100));
    assert_eq!(parse_decimal(".5", 18, 2), Some(50));
    assert_eq!(parse_decimal("5.", 18, 2), Some(500));
    assert_eq!(
        parse_decimal("123456789012345678901234567890123456.78", 38, 2),
        Some(12_345_678_901_234_567_890_123_456_789_012_345_678)
    );
}

#[test]
fn digits_past_the_scale_round_as_duckdb_rounds() {
    assert_eq!(parse_decimal("1.234", 18, 2), Some(123));
    assert_eq!(parse_decimal("1.235", 18, 2), Some(124));
    assert_eq!(parse_decimal("-1.235", 18, 2), Some(-124));
    assert_eq!(parse_decimal("1.5", 18, 0), Some(2));
}

#[test]
fn what_cannot_be_stored_is_refused_not_guessed() {
    for bad in [
        "", " ", "-", ".", "abc", "1,5", "1.2.3", "1e5", "NaN", "--1",
    ] {
        assert_eq!(parse_decimal(bad, 18, 2), None, "{bad:?}");
    }
    assert_eq!(parse_decimal("100.00", 4, 2), None, "needs 5 digits");
    assert_eq!(parse_decimal("99.99", 4, 2), Some(9999));
    assert_eq!(
        parse_decimal("99.995", 4, 2),
        None,
        "rounds past the precision"
    );
    assert_eq!(
        parse_decimal("999999999999999999999999999999999999999999", 38, 0),
        None,
        "past i128 is an answer, not a panic"
    );
}

#[test]
fn a_cell_of_any_number_kind_can_be_stored() {
    assert_eq!(cell_decimal(&CellValue::Int(7), 18, 2), Some(700));
    assert_eq!(cell_decimal(&CellValue::Float(12.34), 18, 2), Some(1234));
    assert_eq!(
        cell_decimal(&CellValue::String("1.50".into()), 18, 2),
        Some(150)
    );
    assert_eq!(cell_decimal(&CellValue::Null, 18, 2), None);
    assert_eq!(cell_decimal(&CellValue::Bool(true), 18, 2), None);
    assert_eq!(cell_decimal(&CellValue::Float(f64::NAN), 18, 2), None);
}

/// A float only where it says the same number, or a save would change the value.
#[test]
fn a_stored_decimal_is_a_float_only_when_that_is_exact() {
    assert_eq!(decimal_cell(1234, 2), CellValue::Float(12.34));
    assert_eq!(decimal_cell(150, 2), CellValue::Float(1.5));
    assert_eq!(
        decimal_cell(12_345_678_901_234_567_890, 2),
        CellValue::String("123456789012345678.90".into())
    );
}

#[test]
fn decimals_compare_by_value() {
    let text = |s: &str| CellValue::String(s.into());
    for (small, large) in [
        ("9.00", "10.00"),
        ("-10.00", "-9.00"),
        ("-1.50", "0.00"),
        ("0.05", "0.5"),
        ("1.2", "1.25"),
        (
            "123456789012345678901234567890123456.77",
            "123456789012345678901234567890123456.78",
        ),
    ] {
        assert_eq!(
            cmp_decimal(&text(small), &text(large)),
            Some(Ordering::Less)
        );
        assert_eq!(
            cmp_decimal(&text(large), &text(small)),
            Some(Ordering::Greater)
        );
    }
    for (a, b) in [("1.50", "1.5"), ("0.00", "-0.00"), ("007", "7.0")] {
        assert_eq!(
            cmp_decimal(&text(a), &text(b)),
            Some(Ordering::Equal),
            "{a} {b}"
        );
    }
    assert_eq!(
        cmp_decimal(&CellValue::Float(9.5), &text("10.00")),
        Some(Ordering::Less),
        "a column can hold both kinds"
    );
    assert_eq!(cmp_decimal(&CellValue::Null, &text("1")), None);
    assert_eq!(cmp_decimal(&text("abc"), &text("1")), None);
}

#[test]
fn adding_by_the_digits_shown_has_no_float_noise() {
    // A float sum gives 0.6000000000000001 here, and it does not equal 0.6.
    assert_ne!(0.1 + 0.2 + 0.3, 0.6);
    let mut sum = Sum::default();
    for v in [0.1, 0.2, 0.3] {
        sum.add_f64(v);
    }
    assert_eq!(sum.value(), 0.6);
    assert_eq!(sum.cell(), Some(CellValue::Float(0.6)));
    assert_eq!(sum_f64([0.1, 0.2, 0.3]), 0.6);
    assert_eq!(mean_f64([0.1, 0.2, 0.3]), 0.2);
}

#[test]
fn a_sum_keeps_digits_no_float_holds() {
    let mut sum = Sum::default();
    for cell in [
        CellValue::String("12345678901234567890.12".into()),
        CellValue::Float(0.01),
        CellValue::Int(1),
    ] {
        assert!(sum.add_cell(&cell));
    }
    assert_eq!(
        sum.cell(),
        Some(CellValue::String("12345678901234567891.13".into()))
    );
    assert!(
        !sum.add_cell(&CellValue::String("abc".into())),
        "text is no number"
    );
    assert!(!sum.add_cell(&CellValue::Null));
    assert_eq!(sum.count(), 3);
}

#[test]
fn whole_sums_are_ints_and_empty_sums_are_none() {
    let mut sum = Sum::default();
    assert_eq!(sum.cell(), None);
    assert_eq!(sum.mean_cell(), None);
    sum.add_cell(&CellValue::Float(1.5));
    sum.add_cell(&CellValue::Float(1.5));
    assert_eq!(sum.cell(), Some(CellValue::Int(3)));
    assert_eq!(sum.mean_cell(), Some(CellValue::Float(1.5)));
}

#[test]
fn a_mean_rounds_its_last_digit_half_away_from_zero() {
    let third = Exact::parse("1").unwrap().div_count(3).unwrap();
    assert_eq!(third.text(), "0.333333333333");
    let two_thirds = Exact::parse("-2").unwrap().div_count(3).unwrap();
    assert_eq!(two_thirds.text(), "-0.666666666667");
    assert_eq!(Exact::parse("1").unwrap().div_count(0), None);
}

#[test]
fn values_no_decimal_holds_fall_back_to_a_careful_float_sum() {
    let mut sum = Sum::default();
    sum.add_f64(1e-300);
    sum.add_f64(1.0);
    assert_eq!(sum.exact(), None);
    assert_eq!(sum.value(), 1.0);
    let mut sum = Sum::default();
    sum.add_cell(&CellValue::String(
        "99999999999999999999999999999999999999".into(),
    ));
    sum.add_cell(&CellValue::String(
        "99999999999999999999999999999999999999".into(),
    ));
    assert_eq!(sum.exact(), None, "past what an i128 holds");
    assert!(sum.value() > 1.9e38);
}

#[test]
fn number_text_becomes_a_float_only_when_the_float_says_the_same() {
    assert_eq!(number_cell("0.1"), Some(CellValue::Float(0.1)));
    assert_eq!(number_cell("1.50"), Some(CellValue::Float(1.5)));
    assert_eq!(
        number_cell("12345678901234567890.12"),
        Some(CellValue::String("12345678901234567890.12".into()))
    );
    for no in ["1e5", " 5", "5 ", "abc", "", ".", "1.2.3", "NaN", "inf"] {
        assert_eq!(number_cell(no), None, "{no:?}");
    }
}

#[test]
fn a_shape_types_a_column_as_narrow_as_its_numbers() {
    let mut shape = DecimalShape::default();
    for cell in [
        CellValue::Float(1.25),
        CellValue::Int(-1234),
        CellValue::String("0.001".into()),
    ] {
        assert!(shape.add(&cell));
    }
    assert!(!shape.add(&CellValue::Null));
    assert!(!shape.add(&CellValue::String("abc".into())));
    assert_eq!(shape.type_name().as_deref(), Some("Decimal128(7, 3)"));

    let mut nan = DecimalShape::default();
    nan.add(&CellValue::Float(f64::NAN));
    assert_eq!(nan.type_name(), None);
    assert_eq!(
        DecimalShape::default().type_name(),
        None,
        "no numbers at all"
    );

    let mut ty = "Decimal128(4, 2)".to_string();
    widen_type(
        &mut ty,
        &[CellValue::Float(123.456), CellValue::String("x".into())],
    );
    assert_eq!(ty, "Decimal128(6, 3)");
    let mut ty = "Decimal128(4, 2)".to_string();
    widen_type(&mut ty, &[CellValue::String("1".repeat(40))]);
    assert_eq!(ty, "Decimal128(4, 2)", "past 38 digits the type stays");
}

#[test]
fn a_server_decimal_reads_as_exactly_the_digits_sent() {
    assert_eq!(server_decimal_cell("42"), CellValue::Int(42));
    assert_eq!(server_decimal_cell("0.05"), CellValue::Float(0.05));
    assert_eq!(server_decimal_cell("99.50"), CellValue::Float(99.5));
    assert_eq!(
        server_decimal_cell("12345678901234567890.12"),
        CellValue::String("12345678901234567890.12".into())
    );
    // Scientific text from a server becomes plain digits, exactly.
    assert_eq!(server_decimal_cell("1.5E+3"), CellValue::Int(1500));
    assert_eq!(server_decimal_cell("1.25E-1"), CellValue::Float(0.125));
    assert_eq!(server_decimal_cell("NaN"), CellValue::String("NaN".into()));
}

fn server_table(cells: Vec<CellValue>) -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "n".into(),
            data_type: SERVER_DECIMAL.into(),
        }],
        rows: cells.into_iter().map(|c| vec![c]).collect(),
        ..DataTable::empty()
    }
}

#[test]
fn a_server_decimal_column_is_sized_by_its_values() {
    let mut t = server_table(vec![
        server_decimal_cell("1.5"),
        server_decimal_cell("-123.25"),
        CellValue::Null,
    ]);
    settle_server_decimals(&mut t);
    assert_eq!(t.columns[0].data_type, "Decimal128(5, 2)");

    let mut whole = server_table(vec![server_decimal_cell("7"), CellValue::Null]);
    settle_server_decimals(&mut whole);
    assert_eq!(whole.columns[0].data_type, "Int64");

    // A NaN makes the column text, every value as the server wrote it.
    let mut nan = server_table(vec![server_decimal_cell("2.5"), server_decimal_cell("NaN")]);
    settle_server_decimals(&mut nan);
    assert_eq!(nan.columns[0].data_type, "Utf8");
    assert_eq!(nan.rows[0][0], CellValue::String("2.5".into()));

    // No rows: the marker's width is the fallback.
    let mut empty = server_table(vec![]);
    settle_server_decimals(&mut empty);
    assert_eq!(empty.columns[0].data_type, SERVER_DECIMAL);
}

#[test]
fn the_same_number_gives_the_same_key() {
    let s = |t: &str| CellValue::String(t.into());
    for one in [CellValue::Int(1), CellValue::Float(1.0), s("1.00"), s("1")] {
        assert_eq!(value_key(&one).as_deref(), Some("1"), "{one:?}");
    }
    assert_eq!(value_key(&s("-0.50")).as_deref(), Some("-0.5"));
    assert_eq!(value_key(&s("abc")), None);
}

fn ex(s: &str) -> Exact {
    Exact::parse(s).unwrap()
}

#[test]
fn multiplying_is_exact() {
    assert_eq!(
        ex("19.99").checked_mul(ex("1.1052")).unwrap().text(),
        "22.092948"
    );
    assert_eq!(ex("-2.5").checked_mul(ex("4")).unwrap().text(), "-10.0");
}

#[test]
fn dividing_rounds_the_last_digit_half_away_from_zero() {
    assert_eq!(ex("1").checked_div(ex("3"), 4).unwrap().text(), "0.3333");
    assert_eq!(ex("2").checked_div(ex("3"), 4).unwrap().text(), "0.6667");
    assert_eq!(ex("-2").checked_div(ex("3"), 4).unwrap().text(), "-0.6667");
    assert_eq!(ex("10").checked_div(ex("0.5"), 0).unwrap().text(), "20");
    assert!(ex("1").checked_div(ex("0"), 2).is_none());
}

#[test]
fn an_unrounded_division_keeps_what_fits_and_drops_trailing_zeros() {
    assert_eq!(
        ex("1")
            .div_up_to(ex("4"), UNROUNDED_DECIMALS)
            .unwrap()
            .text(),
        "0.25"
    );
    let third = ex("1").div_up_to(ex("3"), UNROUNDED_DECIMALS).unwrap();
    assert_eq!(third.scale(), 20);
    // A huge numerator cannot carry 20 decimals in 128 bits; fewer are kept.
    let big = ex("12345678901234567890")
        .div_up_to(ex("7"), UNROUNDED_DECIMALS)
        .unwrap();
    assert!(big.scale() < 20);
}

#[test]
fn rounding_goes_half_away_from_zero_and_never_adds_digits() {
    assert_eq!(ex("2.345").round_to(2).text(), "2.35");
    assert_eq!(ex("-2.345").round_to(2).text(), "-2.35");
    assert_eq!(ex("2.344").round_to(2).text(), "2.34");
    assert_eq!(ex("1234.5").round_to(0).text(), "1235");
    assert_eq!(ex("2.3").round_to(4).text(), "2.3");
}

#[test]
fn raw_values_round_trip_through_a_scale() {
    let v = Exact::from_raw(51_4333, 4);
    assert_eq!(v.text(), "51.4333");
    assert_eq!(v.raw_scaled(6), Some(51_433_300));
    assert_eq!(ex("1.5").raw_scaled(0), None);
}
