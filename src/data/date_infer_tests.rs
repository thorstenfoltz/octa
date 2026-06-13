//! Unit tests for [`date_infer`](date_infer). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;
use std::collections::HashMap;

fn table(values: Vec<&str>) -> DataTable {
    let rows: Vec<Vec<CellValue>> = values
        .into_iter()
        .map(|s| {
            vec![if s.is_empty() {
                CellValue::Null
            } else {
                CellValue::String(s.to_string())
            }]
        })
        .collect();
    DataTable {
        columns: vec![ColumnInfo {
            name: "d".to_string(),
            data_type: "Utf8".to_string(),
        }],
        rows,
        edits: HashMap::new(),
        ..DataTable::empty()
    }
}

#[test]
fn iso_dash_promoted() {
    let strings = vec![Some("2024-03-15"), Some("2024-04-01"), None];
    match infer_column(&strings) {
        InferOutcome::PromotedDate(DateLayout::YmdDash) => {}
        other => panic!("expected YmdDash, got {other:?}"),
    }
}

#[test]
fn unambiguous_european_when_first_part_over_12() {
    let strings = vec![Some("15/04/2024"), Some("03/01/2025")];
    match infer_column(&strings) {
        InferOutcome::PromotedDate(DateLayout::DmySlash) => {}
        other => panic!("expected DmySlash, got {other:?}"),
    }
}

#[test]
fn unambiguous_us_when_second_part_over_12() {
    let strings = vec![Some("04/15/2024"), Some("12/31/2025")];
    match infer_column(&strings) {
        InferOutcome::PromotedDate(DateLayout::MdySlash) => {}
        other => panic!("expected MdySlash, got {other:?}"),
    }
}

#[test]
fn ambiguous_when_all_components_below_13() {
    let strings = vec![Some("02/03/2024"), Some("04/05/2025")];
    match infer_column(&strings) {
        InferOutcome::AmbiguousDate { candidates, .. } => {
            assert!(candidates.contains(&DateLayout::DmySlash));
            assert!(candidates.contains(&DateLayout::MdySlash));
        }
        other => panic!("expected AmbiguousDate, got {other:?}"),
    }
}

#[test]
fn dot_separated_european() {
    let strings = vec![Some("15.04.2024"), Some("01.12.2025")];
    match infer_column(&strings) {
        InferOutcome::PromotedDate(DateLayout::DmyDot) => {}
        other => panic!("expected DmyDot, got {other:?}"),
    }
}

#[test]
fn mixed_strings_skip() {
    let strings = vec![Some("hello"), Some("2024-03-15")];
    assert!(matches!(infer_column(&strings), InferOutcome::Skip));
}

#[test]
fn null_only_skip() {
    let strings: Vec<Option<&str>> = vec![None, None];
    assert!(matches!(infer_column(&strings), InferOutcome::Skip));
}

#[test]
fn near_miss_reports_failed() {
    // Three of four parse as YYYY-MM-DD; one is junk -> Failed, not Skip.
    let strings = vec![
        Some("2024-03-15"),
        Some("2024-04-01"),
        Some("2024-05-20"),
        Some("not-a-date"),
    ];
    match infer_column(&strings) {
        InferOutcome::Failed {
            parsed,
            total,
            failures,
            ..
        } => {
            assert_eq!(parsed, 3);
            assert_eq!(total, 4);
            assert_eq!(failures, vec!["not-a-date".to_string()]);
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn half_match_is_skip_not_failed() {
    // Exactly 50% parse -> not a majority -> Skip (avoids false alarms).
    let strings = vec![
        Some("hello"),
        Some("world"),
        Some("2024-03-15"),
        Some("foo"),
    ];
    assert!(matches!(infer_column(&strings), InferOutcome::Skip));
}

#[test]
fn datetime_iso_t() {
    let strings = vec![Some("2024-03-15T13:45:00"), Some("2024-04-01T08:00:00")];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashT) => {}
        other => panic!("expected YmdDashT, got {other:?}"),
    }
}

#[test]
fn apply_promotes_cells() {
    let mut t = table(vec!["15.04.2024", "01.12.2025", ""]);
    apply_date(&mut t, 0, DateLayout::DmyDot);
    assert_eq!(t.columns[0].data_type, "Date32");
    match t.rows[0][0] {
        CellValue::Date(ref s) => assert_eq!(s, "2024-04-15"),
        ref other => panic!("expected Date, got {other:?}"),
    }
    assert!(matches!(t.rows[2][0], CellValue::Null));
}

#[test]
fn apply_datetime_promotes_cells() {
    let strings = vec![Some("2024-03-15T13:45:00"), Some("2024-04-01T08:00:00")];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(layout) => {
            let mut t = table(vec!["2024-03-15T13:45:00", "2024-04-01T08:00:00"]);
            apply_datetime(&mut t, 0, layout);
            assert_eq!(t.columns[0].data_type, "Timestamp(Microsecond, None)");
            match t.rows[0][0] {
                CellValue::DateTime(ref s) => {
                    assert_eq!(s, "2024-03-15 13:45:00");
                }
                ref other => panic!("expected DateTime, got {other:?}"),
            }
        }
        other => panic!("expected PromotedDateTime, got {other:?}"),
    }
}

#[test]
fn iso_utc_zulu_normalized_to_utc() {
    let strings = vec![
        Some("2024-03-15T13:45:00Z"),
        Some("2024-04-01T08:00:00z"), // lowercase z also accepted
    ];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashTTz) => {}
        other => panic!("expected YmdDashTTz, got {other:?}"),
    }
    // Round-trip via apply: cells render as UTC, no tz suffix.
    let mut t = table(vec!["2024-03-15T13:45:00Z", "2024-04-01T08:00:00z"]);
    apply_datetime(&mut t, 0, DateTimeLayout::YmdDashTTz);
    match t.rows[0][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-03-15 13:45:00"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
    match t.rows[1][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-04-01 08:00:00"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
}

#[test]
fn iso_offset_shifted_to_utc() {
    // +02:00 shifts back two hours; -05:00 shifts forward five.
    let strings = vec![
        Some("2024-03-15T14:30:00+02:00"),
        Some("2024-03-15T09:00:00-05:00"),
    ];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashTTz) => {}
        other => panic!("expected YmdDashTTz, got {other:?}"),
    }
    let mut t = table(vec![
        "2024-03-15T14:30:00+02:00",
        "2024-03-15T09:00:00-05:00",
    ]);
    apply_datetime(&mut t, 0, DateTimeLayout::YmdDashTTz);
    match t.rows[0][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-03-15 12:30:00"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
    match t.rows[1][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-03-15 14:00:00"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
}

#[test]
fn iso_offset_with_fractional_preserved() {
    let strings = vec![
        Some("2024-03-15T14:30:00.123456+02:00"),
        Some("2024-04-01T08:00:00.5Z"),
    ];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashTTz) => {}
        other => panic!("expected YmdDashTTz, got {other:?}"),
    }
    let mut t = table(vec![
        "2024-03-15T14:30:00.123456+02:00",
        "2024-04-01T08:00:00.5Z",
    ]);
    apply_datetime(&mut t, 0, DateTimeLayout::YmdDashTTz);
    match t.rows[0][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-03-15 12:30:00.123456"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
    match t.rows[1][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, "2024-04-01 08:00:00.500"),
        ref other => panic!("expected DateTime, got {other:?}"),
    }
}

#[test]
fn iso_offset_compact_form_accepted() {
    // `+0200` (no colon) is the RFC 822 / ISO 8601 basic offset form.
    let strings = vec![
        Some("2024-03-15T14:30:00+0200"),
        Some("2024-04-01T08:00:00-0500"),
    ];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashTTz) => {}
        other => panic!("expected YmdDashTTz, got {other:?}"),
    }
}

#[test]
fn mixed_naive_and_tz_rejects() {
    // A column with one tz-aware and one naive row is semantically
    // inconsistent - neither layout matches every value.
    let strings = vec![Some("2024-03-15T14:30:00Z"), Some("2024-04-01T08:00:00")];
    assert!(matches!(infer_column(&strings), InferOutcome::Skip));
}

#[test]
fn fractional_seconds_preserved_after_promotion() {
    // Mixed precision in the same column: milli, micro, nano, and
    // whole-second values must round-trip through the inference pass.
    let strings = vec![
        Some("2024-03-15 13:45:00.123"),
        Some("2024-04-01 08:00:00.456789"),
        Some("2024-05-10 09:15:30.000000001"),
        Some("2024-06-20 12:00:00"),
    ];
    match infer_column(&strings) {
        InferOutcome::PromotedDateTime(DateTimeLayout::YmdDashSpace) => {}
        other => panic!("expected YmdDashSpace, got {other:?}"),
    }
    let mut t = table(vec![
        "2024-03-15 13:45:00.123",
        "2024-04-01 08:00:00.456789",
        "2024-05-10 09:15:30.000000001",
        "2024-06-20 12:00:00",
    ]);
    apply_datetime(&mut t, 0, DateTimeLayout::YmdDashSpace);
    let expect = |row: usize, want: &str| match t.rows[row][0] {
        CellValue::DateTime(ref s) => assert_eq!(s, want, "row {row}"),
        ref other => panic!("row {row} expected DateTime, got {other:?}"),
    };
    expect(0, "2024-03-15 13:45:00.123");
    expect(1, "2024-04-01 08:00:00.456789");
    expect(2, "2024-05-10 09:15:30.000000001");
    expect(3, "2024-06-20 12:00:00");
}
