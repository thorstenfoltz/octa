//! Unit tests for [`num_format`](num_format). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn nf(decimals: Option<i32>, rounding: RoundingMode) -> NumberFormat {
    NumberFormat { decimals, rounding }
}

const EN: SeparatorStyle = SeparatorStyle::English;
const EU: SeparatorStyle = SeparatorStyle::European;

#[test]
fn groups_integers() {
    assert_eq!(
        format_cell_number(&CellValue::Int(1234567), None, true, EN).unwrap(),
        "1,234,567"
    );
    assert_eq!(
        format_cell_number(&CellValue::Int(-42000), None, true, EN).unwrap(),
        "-42,000"
    );
    assert_eq!(
        format_cell_number(&CellValue::Int(999), None, true, EN).unwrap(),
        "999"
    );
}

#[test]
fn european_style() {
    assert_eq!(
        format_cell_number(&CellValue::Float(1234567.89), None, true, EU).unwrap(),
        "1.234.567,89"
    );
    // Decimal mark switches even with grouping off.
    assert_eq!(
        format_cell_number(&CellValue::Float(1234.5), None, false, EU).unwrap(),
        "1234,5"
    );
}

#[test]
fn grouping_off_leaves_plain() {
    assert_eq!(
        format_cell_number(&CellValue::Int(1234567), None, false, EN).unwrap(),
        "1234567"
    );
}

#[test]
fn groups_float_integer_part_only() {
    assert_eq!(
        format_cell_number(&CellValue::Float(1234567.89), None, true, EN).unwrap(),
        "1,234,567.89"
    );
}

#[test]
fn fixed_decimals_pad_with_zeros() {
    let f = nf(Some(2), RoundingMode::Normal);
    assert_eq!(
        format_cell_number(&CellValue::Float(2.5), Some(f), false, EN).unwrap(),
        "2.50"
    );
    assert_eq!(
        format_cell_number(&CellValue::Int(3), Some(f), false, EN).unwrap(),
        "3.00"
    );
}

#[test]
fn rounding_modes() {
    let normal = nf(Some(2), RoundingMode::Normal);
    let up = nf(Some(2), RoundingMode::Up);
    let down = nf(Some(2), RoundingMode::Down);
    assert_eq!(round_value(1.45678, normal), 1.46);
    assert_eq!(round_value(1.45678, up), 1.46);
    assert_eq!(round_value(1.45123, up), 1.46);
    assert_eq!(round_value(1.45678, down), 1.45);
    // Negative numbers: Up = toward +inf, Down = toward -inf.
    assert_eq!(round_value(-1.231, up), -1.23);
    assert_eq!(round_value(-1.231, down), -1.24);
    // Half away from zero.
    assert_eq!(round_value(2.5, nf(Some(0), RoundingMode::Normal)), 3.0);
    assert_eq!(round_value(-2.5, nf(Some(0), RoundingMode::Normal)), -3.0);
}

#[test]
fn negative_decimals_round_before_point() {
    // Round to the nearest 100.
    let f = nf(Some(-2), RoundingMode::Normal);
    assert_eq!(round_value(1234.5, f), 1200.0);
    assert_eq!(
        format_cell_number(&CellValue::Float(1234.5), Some(f), true, EN).unwrap(),
        "1,200"
    );
    assert_eq!(
        format_cell_number(&CellValue::Int(1789), Some(f), false, EN).unwrap(),
        "1800"
    );
}

#[test]
fn rounding_and_grouping_compose() {
    let f = nf(Some(2), RoundingMode::Normal);
    assert_eq!(
        format_cell_number(&CellValue::Float(1234567.899), Some(f), true, EN).unwrap(),
        "1,234,567.90"
    );
}

#[test]
fn non_numeric_returns_none() {
    assert!(format_cell_number(&CellValue::String("hi".into()), None, true, EN).is_none());
    assert!(format_cell_number(&CellValue::Null, None, true, EN).is_none());
}

#[test]
fn non_finite_floats_pass_through() {
    assert_eq!(
        format_cell_number(&CellValue::Float(f64::NAN), None, true, EN).unwrap(),
        "NaN"
    );
    assert_eq!(
        round_value(f64::INFINITY, nf(Some(2), RoundingMode::Normal)),
        f64::INFINITY
    );
}
