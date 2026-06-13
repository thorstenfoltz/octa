//! Unit tests for [`time_calc`](time_calc). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn date(s: &str) -> CellValue {
    CellValue::Date(s.to_string())
}
fn datetime(s: &str) -> CellValue {
    CellValue::DateTime(s.to_string())
}

#[test]
fn difference_in_days() {
    let out = evaluate_cell(
        TimeCalcOp::Difference {
            unit: TimeUnit::Days,
        },
        &date("2024-01-01"),
        Some(&date("2024-01-11")),
    );
    assert_eq!(out, Some(CellValue::Float(10.0)));
}

#[test]
fn difference_in_hours() {
    let out = evaluate_cell(
        TimeCalcOp::Difference {
            unit: TimeUnit::Hours,
        },
        &datetime("2024-01-01 00:00:00"),
        Some(&datetime("2024-01-01 06:30:00")),
    );
    assert_eq!(out, Some(CellValue::Float(6.5)));
}

#[test]
fn difference_in_months() {
    let out = evaluate_cell(
        TimeCalcOp::Difference {
            unit: TimeUnit::Months,
        },
        &date("2024-01-15"),
        Some(&date("2024-04-15")),
    );
    assert_eq!(out, Some(CellValue::Float(3.0)));
}

#[test]
fn convert_ms_to_seconds() {
    let out = evaluate_cell(
        TimeCalcOp::ConvertDuration {
            from: TimeUnit::Milliseconds,
            to: TimeUnit::Seconds,
        },
        &CellValue::Int(90_000),
        None,
    );
    assert_eq!(out, Some(CellValue::Float(90.0)));
}

#[test]
fn convert_seconds_to_minutes() {
    let out = evaluate_cell(
        TimeCalcOp::ConvertDuration {
            from: TimeUnit::Seconds,
            to: TimeUnit::Minutes,
        },
        &CellValue::Float(150.0),
        None,
    );
    assert_eq!(out, Some(CellValue::Float(2.5)));
}

#[test]
fn add_months_clamps_day() {
    // Jan 31 + 1 month -> Feb 29 (2024 is a leap year), stays a Date.
    let out = evaluate_cell(
        TimeCalcOp::AddSubtract {
            unit: TimeUnit::Months,
            amount: 1,
        },
        &date("2024-01-31"),
        None,
    );
    assert_eq!(out, Some(CellValue::Date("2024-02-29".to_string())));
}

#[test]
fn add_days_to_date_stays_date() {
    let out = evaluate_cell(
        TimeCalcOp::AddSubtract {
            unit: TimeUnit::Days,
            amount: 5,
        },
        &date("2024-01-01"),
        None,
    );
    assert_eq!(out, Some(CellValue::Date("2024-01-06".to_string())));
}

#[test]
fn add_hours_promotes_date_to_datetime() {
    let out = evaluate_cell(
        TimeCalcOp::AddSubtract {
            unit: TimeUnit::Hours,
            amount: 26,
        },
        &date("2024-01-01"),
        None,
    );
    assert_eq!(
        out,
        Some(CellValue::DateTime("2024-01-02 02:00:00".to_string()))
    );
}

#[test]
fn subtract_years() {
    let out = evaluate_cell(
        TimeCalcOp::AddSubtract {
            unit: TimeUnit::Years,
            amount: -2,
        },
        &date("2024-06-15"),
        None,
    );
    assert_eq!(out, Some(CellValue::Date("2022-06-15".to_string())));
}

#[test]
fn extract_parts() {
    let dt = datetime("2024-03-09 14:25:36");
    assert_eq!(
        evaluate_cell(
            TimeCalcOp::Extract {
                component: DateComponent::Year
            },
            &dt,
            None
        ),
        Some(CellValue::Int(2024))
    );
    assert_eq!(
        evaluate_cell(
            TimeCalcOp::Extract {
                component: DateComponent::Month
            },
            &dt,
            None
        ),
        Some(CellValue::Int(3))
    );
    assert_eq!(
        evaluate_cell(
            TimeCalcOp::Extract {
                component: DateComponent::Hour
            },
            &dt,
            None
        ),
        Some(CellValue::Int(14))
    );
    // 2024-03-09 is a Saturday -> ISO weekday 6.
    assert_eq!(
        evaluate_cell(
            TimeCalcOp::Extract {
                component: DateComponent::Weekday
            },
            &dt,
            None
        ),
        Some(CellValue::Int(6))
    );
}

#[test]
fn non_date_returns_none() {
    let out = evaluate_cell(
        TimeCalcOp::Extract {
            component: DateComponent::Year,
        },
        &CellValue::String("not a date".to_string()),
        None,
    );
    assert_eq!(out, None);
}

#[test]
fn unix_seconds_to_datetime() {
    let out = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::ToDateTime,
            unit: UnixUnit::Seconds,
        },
        &CellValue::Int(1_700_000_000),
        None,
    );
    assert_eq!(
        out,
        Some(CellValue::DateTime("2023-11-14 22:13:20".to_string()))
    );
}

#[test]
fn unix_millis_to_datetime_keeps_fraction() {
    let out = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::ToDateTime,
            unit: UnixUnit::Milliseconds,
        },
        &CellValue::Int(1_700_000_000_500),
        None,
    );
    assert_eq!(
        out,
        Some(CellValue::DateTime("2023-11-14 22:13:20.500".to_string()))
    );
}

#[test]
fn datetime_to_unix_seconds() {
    let out = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::FromDateTime,
            unit: UnixUnit::Seconds,
        },
        &datetime("2023-11-14 22:13:20"),
        None,
    );
    assert_eq!(out, Some(CellValue::Int(1_700_000_000)));
}

#[test]
fn unix_roundtrips_nanoseconds() {
    // A nanosecond epoch exceeds f64's exact-integer range, so the i128
    // path must preserve it exactly across the round trip.
    let epoch = 1_700_000_000_123_456_789i64;
    let dt = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::ToDateTime,
            unit: UnixUnit::Nanoseconds,
        },
        &CellValue::Int(epoch),
        None,
    )
    .unwrap();
    let back = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::FromDateTime,
            unit: UnixUnit::Nanoseconds,
        },
        &dt,
        None,
    );
    assert_eq!(back, Some(CellValue::Int(epoch)));
}

#[test]
fn unix_to_datetime_reads_epoch_from_timestamp_typed_cell() {
    // Regression: a source can type an epoch-microsecond column as
    // Timestamp(...) while the cell still holds the raw number as text
    // (CellValue::DateTime). ToDateTime must read the number out of it
    // rather than returning None for every unit.
    let cell = CellValue::DateTime("1769975775172766".to_string());
    let dt = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::ToDateTime,
            unit: UnixUnit::Microseconds,
        },
        &cell,
        None,
    )
    .expect("epoch in a DateTime-typed cell should convert");
    // Round-trips back to the same epoch number.
    let back = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::FromDateTime,
            unit: UnixUnit::Microseconds,
        },
        &dt,
        None,
    );
    assert_eq!(back, Some(CellValue::Int(1_769_975_775_172_766)));
}

#[test]
fn unix_to_datetime_rejects_non_numeric() {
    let out = evaluate_cell(
        TimeCalcOp::UnixConvert {
            direction: UnixDirection::ToDateTime,
            unit: UnixUnit::Seconds,
        },
        &CellValue::String("not a number".to_string()),
        None,
    );
    assert_eq!(out, None);
}

#[test]
fn parses_european_string_dates() {
    // Dotted European layout via date_infer fallback.
    let out = evaluate_cell(
        TimeCalcOp::Difference {
            unit: TimeUnit::Days,
        },
        &CellValue::String("01.01.2024".to_string()),
        Some(&CellValue::String("08.01.2024".to_string())),
    );
    assert_eq!(out, Some(CellValue::Float(7.0)));
}
