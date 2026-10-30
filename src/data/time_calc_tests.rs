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

// --- Timezone conversion -------------------------------------------------
//
// Octa datetimes are `Timestamp(Microsecond, None)`, i.e. always naive, so both
// daylight-saving edges are reachable and neither has a single correct answer.
// These pin the refusal rather than a guess.

#[test]
fn timezone_converts_a_plain_instant() {
    let v = CellValue::DateTime("2024-01-15 12:00:00".to_string());
    match convert_cell_timezone(&v, chrono_tz::UTC, chrono_tz::Asia::Tokyo) {
        TzOutcome::Converted(dt) => assert_eq!(format_datetime(dt), "2024-01-15 21:00:00"),
        other => panic!("expected Converted, got {other:?}"),
    }
}

#[test]
fn autumn_overlap_is_ambiguous_not_guessed() {
    // 02:30 on 2024-10-27 happens twice in Europe/Berlin.
    let v = CellValue::DateTime("2024-10-27 02:30:00".to_string());
    let got = convert_cell_timezone(&v, chrono_tz::Europe::Berlin, chrono_tz::UTC);
    assert!(matches!(got, TzOutcome::Ambiguous), "got {got:?}");
}

#[test]
fn spring_gap_is_ambiguous_not_guessed() {
    // 02:30 on 2024-03-31 never happens in Europe/Berlin.
    let v = CellValue::DateTime("2024-03-31 02:30:00".to_string());
    let got = convert_cell_timezone(&v, chrono_tz::Europe::Berlin, chrono_tz::UTC);
    assert!(matches!(got, TzOutcome::Ambiguous), "got {got:?}");
}

#[test]
fn non_datetime_is_reported_separately_from_ambiguous() {
    let v = CellValue::String("not a date".to_string());
    let got = convert_cell_timezone(&v, chrono_tz::UTC, chrono_tz::UTC);
    assert!(matches!(got, TzOutcome::NotADateTime), "got {got:?}");
}

#[test]
fn column_conversion_counts_ambiguous_and_nulls_them() {
    let vals = vec![
        CellValue::DateTime("2024-01-15 12:00:00".to_string()), // fine
        CellValue::DateTime("2024-10-27 02:30:00".to_string()), // DST overlap
        CellValue::String("junk".to_string()),                  // not a date
    ];
    let (out, ambiguous) =
        convert_timezone_column(&vals, chrono_tz::Europe::Berlin, chrono_tz::UTC);
    assert_eq!(
        ambiguous, 1,
        "only the DST overlap counts, not the junk cell"
    );
    assert_eq!(out.len(), 3);
    assert!(matches!(out[0], CellValue::DateTime(_)));
    assert!(matches!(out[1], CellValue::Null));
    assert!(matches!(out[2], CellValue::Null));
}

const H: i64 = 3_600_000_000_000;

#[test]
fn difference_of_two_times_as_a_duration_with_next_day() {
    let op = TimeCalcOp::DurationBetween { next_day: true };
    assert_eq!(
        evaluate_cell(op, &CellValue::Time(22 * H), Some(&CellValue::Time(6 * H))),
        Some(CellValue::Duration(8 * H))
    );
    let op = TimeCalcOp::DurationBetween { next_day: false };
    assert_eq!(
        evaluate_cell(op, &CellValue::Time(22 * H), Some(&CellValue::Time(6 * H))),
        Some(CellValue::Duration(-16 * H))
    );
    let (a, b) = (
        datetime("2024-01-01 22:00:00"),
        datetime("2024-01-03 06:30:00"),
    );
    assert_eq!(
        evaluate_cell(op, &a, Some(&b)),
        Some(CellValue::Duration(32 * H + H / 2))
    );
    // Two dates: whole days. A time against a date: no answer.
    assert_eq!(
        evaluate_cell(op, &date("2024-01-01"), Some(&date("2024-01-03"))),
        Some(CellValue::Duration(48 * H))
    );
    assert_eq!(
        evaluate_cell(op, &CellValue::Time(H), Some(&date("2024-01-03"))),
        None
    );
}

#[test]
fn the_number_difference_reads_two_times() {
    let op = TimeCalcOp::Difference {
        unit: TimeUnit::Minutes,
    };
    assert_eq!(
        evaluate_cell(
            op,
            &CellValue::Time(8 * H),
            Some(&CellValue::Time(9 * H + H / 2))
        ),
        Some(CellValue::Float(90.0))
    );
}

#[test]
fn adding_a_duration_wraps_a_time_at_midnight_and_moves_a_datetime() {
    let op = TimeCalcOp::AddDuration { amount: None };
    assert_eq!(
        evaluate_cell(
            op,
            &CellValue::Time(22 * H),
            Some(&CellValue::Duration(3 * H))
        ),
        Some(CellValue::Time(H))
    );
    assert_eq!(
        evaluate_cell(
            op,
            &datetime("2024-01-01 22:00:00"),
            Some(&CellValue::Duration(3 * H))
        ),
        Some(datetime("2024-01-02 01:00:00"))
    );
    // A date becomes a datetime, so a column never mixes the two (whole days
    // onto a date is Add / subtract).
    assert_eq!(
        evaluate_cell(op, &date("2024-01-01"), Some(&CellValue::Duration(48 * H))),
        Some(datetime("2024-01-03 00:00:00"))
    );
    assert_eq!(
        evaluate_cell(op, &date("2024-01-01"), Some(&CellValue::Duration(-H))),
        Some(datetime("2023-12-31 23:00:00"))
    );
    let op = TimeCalcOp::AddDuration { amount: Some(-H) };
    assert_eq!(
        evaluate_cell(op, &CellValue::Time(0), None),
        Some(CellValue::Time(23 * H))
    );
}

#[test]
fn extract_hour_and_convert_both_ways() {
    let ex = TimeCalcOp::Extract {
        component: DateComponent::Hour,
    };
    assert_eq!(
        evaluate_cell(ex, &CellValue::Time(14 * H + H / 2), None),
        Some(CellValue::Int(14))
    );
    let year = TimeCalcOp::Extract {
        component: DateComponent::Year,
    };
    assert_eq!(evaluate_cell(year, &CellValue::Time(H), None), None);
    let to_hours = TimeCalcOp::DurationToNumber {
        unit: TimeUnit::Hours,
    };
    assert_eq!(
        evaluate_cell(to_hours, &CellValue::Duration(30 * H + H / 4), None),
        Some(CellValue::Float(30.25))
    );
    let from_minutes = TimeCalcOp::NumberToDuration {
        unit: TimeUnit::Minutes,
    };
    assert_eq!(
        evaluate_cell(from_minutes, &CellValue::Int(90), None),
        Some(CellValue::Duration(H + H / 2))
    );
    // Months have no fixed length.
    let months = TimeCalcOp::NumberToDuration {
        unit: TimeUnit::Months,
    };
    assert_eq!(evaluate_cell(months, &CellValue::Int(1), None), None);
}

#[test]
fn the_new_ops_name_their_inputs_and_result_types() {
    assert!(TimeCalcOp::DurationBetween { next_day: false }.needs_second_input());
    assert!(TimeCalcOp::AddDuration { amount: None }.needs_second_input());
    assert!(!TimeCalcOp::AddDuration { amount: Some(H) }.needs_second_input());
    assert_eq!(
        result_type_name(TimeCalcOp::NumberToDuration {
            unit: TimeUnit::Hours
        }),
        "Duration(Microsecond)"
    );
    assert_eq!(cell_arrow_type(&CellValue::Time(H)), "Time64(Microsecond)");
    assert_eq!(
        cell_arrow_type(&CellValue::Duration(1)),
        "Duration(Nanosecond)"
    );
}
