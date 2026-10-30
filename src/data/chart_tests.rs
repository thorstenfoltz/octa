//! Unit tests for [`chart`](chart). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn sturges_clamps_low_and_high() {
    assert_eq!(sturges_bins(0), 1);
    assert!(sturges_bins(10) >= 5);
    assert!(sturges_bins(1_000_000) <= 50);
}

#[test]
fn quantile_picks_midpoint() {
    let v = vec![1.0, 2.0, 3.0, 4.0];
    assert!((quantile(&v, 0.5) - 2.5).abs() < 1e-9);
}

#[test]
fn a_tick_before_1970_lands_on_the_earlier_second() {
    // trunc() read -1.5 as second -1 plus half a second: one second late.
    assert_eq!(format_seconds_as_datetime(-1.5), "1969-12-31 23:59:58");
}

/// Time and Duration chart as seconds on a numeric axis whose ticks read as
/// clock times / durations, with seconds only when ticks are under a minute
/// apart; a Time X axis spans the day by default.
#[test]
fn time_and_duration_are_numeric_axes_with_clock_labels() {
    use crate::data::times::NANOS_PER_SEC;
    assert_eq!(
        cell_to_f64(&CellValue::Time(3_600 * NANOS_PER_SEC)),
        Some(3_600.0)
    );
    assert_eq!(
        cell_to_f64(&CellValue::Duration(-90 * NANOS_PER_SEC)),
        Some(-90.0)
    );
    assert_eq!(
        format_clock_tick(XAxisKind::Time, 52_200.0, 3_600.0),
        "14:30"
    );
    assert_eq!(
        format_clock_tick(XAxisKind::Time, 52_215.0, 15.0),
        "14:30:15"
    );
    assert_eq!(
        format_clock_tick(XAxisKind::Duration, 108_900.0, 3_600.0),
        "30:15"
    );
    assert_eq!(
        format_clock_tick(XAxisKind::Duration, -1_800.0, 600.0),
        "-00:30"
    );
    assert_eq!(default_x_range(XAxisKind::Time), Some((0.0, 86_400.0)));
    assert_eq!(default_x_range(XAxisKind::Duration), None);

    let h = 3_600 * NANOS_PER_SEC;
    let table = DataTable {
        columns: vec![
            crate::data::ColumnInfo {
                name: "clock_in".into(),
                data_type: "Time64(µs)".into(),
            },
            crate::data::ColumnInfo {
                name: "worked".into(),
                data_type: "Duration(µs)".into(),
            },
        ],
        rows: (6..10)
            .map(|i| vec![CellValue::Time(i * h), CellValue::Duration(8 * h + i)])
            .collect(),
        ..DataTable::empty()
    };
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..ChartConfig::default()
    };
    let prep = build_chart(&table, &[0, 1, 2, 3], &cfg, ChartLimits::default()).unwrap();
    assert_eq!(prep.x_axis_kind, XAxisKind::Time);
    assert_eq!(prep.y_axis_kind, XAxisKind::Duration);
    assert!(is_chart_numeric("Duration(ns)") && is_chart_numeric("Time64(µs)"));
    assert!(has_numeric_column(&table));
}
