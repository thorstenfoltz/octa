//! Unit tests for [`chart`](chart). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::ColumnInfo;

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

fn heat_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [("region", "Utf8"), ("month", "Utf8"), ("sales", "Int64")]
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).into(),
            data_type: (*ty).into(),
        })
        .collect();
    let s = |v: &str| CellValue::String(v.into());
    t.rows = vec![
        vec![s("north"), s("jan"), CellValue::Int(10)],
        vec![s("north"), s("jan"), CellValue::Int(5)],
        vec![s("south"), s("feb"), CellValue::Int(7)],
        vec![CellValue::Null, s("feb"), CellValue::Int(1)],
    ];
    t
}

#[test]
fn a_heatmap_sums_each_pair_and_leaves_missing_pairs_empty() {
    let t = heat_table();
    let cfg = ChartConfig {
        kind: ChartKind::Heatmap,
        x_col: Some(0),
        row_col: Some(1),
        y_cols: vec![2],
        agg: Aggregation::Sum,
        ..ChartConfig::default()
    };
    let prep = build_chart(&t, &[0, 1, 2, 3], &cfg, ChartLimits::default()).unwrap();
    let ChartData::Heatmap {
        x_categories,
        y_categories,
        values,
        counts,
    } = prep.data
    else {
        panic!("not a heatmap")
    };
    assert_eq!(x_categories, ["north", "south", "(null)"]);
    assert_eq!(y_categories, ["feb", "jan"]);
    assert_eq!(values[1][0], Some(15.0)); // jan x north
    assert_eq!(counts[1][0], 2);
    assert_eq!(values[1][1], None); // jan x south: no rows
    assert_eq!(values[0][2], Some(1.0)); // feb x (null)
}

#[test]
fn a_heatmap_counts_rows_without_a_value_column() {
    let t = heat_table();
    let cfg = ChartConfig {
        kind: ChartKind::Heatmap,
        x_col: Some(0),
        row_col: Some(1),
        agg: Aggregation::Count,
        ..ChartConfig::default()
    };
    let prep = build_chart(&t, &[0, 1, 2, 3], &cfg, ChartLimits::default()).unwrap();
    let ChartData::Heatmap { values, .. } = prep.data else {
        panic!()
    };
    assert_eq!(values[1][0], Some(2.0));
}

#[test]
fn each_heatmap_axis_is_capped() {
    let t = heat_table();
    let cfg = ChartConfig {
        kind: ChartKind::Heatmap,
        x_col: Some(0),
        row_col: Some(1),
        agg: Aggregation::Count,
        ..ChartConfig::default()
    };
    let limits = ChartLimits {
        max_categories: 2,
        ..ChartLimits::default()
    };
    assert_eq!(
        build_chart(&t, &[0, 1, 2, 3], &cfg, limits).unwrap_err(),
        ChartError::TooManyCategories { count: 3, cap: 2 }
    );
    let cfg = ChartConfig {
        x_col: Some(1),
        row_col: Some(0),
        ..cfg
    };
    assert_eq!(
        build_chart(&t, &[0, 1, 2, 3], &cfg, limits).unwrap_err(),
        ChartError::TooManyRowCategories { count: 3, cap: 2 }
    );
}

fn days(y: i32, m: u32, d: u32) -> i64 {
    (chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
        - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
    .num_days()
}

#[test]
fn calendar_cells_put_monday_first_and_week_one_on_new_year() {
    assert_eq!(calendar_cell(0), Some((1970, 0, 3))); // a Thursday
    assert_eq!(calendar_cell(days(2024, 1, 1)), Some((2024, 0, 0))); // Monday
    assert_eq!(calendar_cell(days(2024, 1, 7)), Some((2024, 0, 6))); // Sunday
    assert_eq!(calendar_cell(days(2024, 1, 8)), Some((2024, 1, 0)));
    // Year boundary: 2023 ends on a Sunday in its last column.
    assert_eq!(calendar_cell(days(2023, 12, 31)), Some((2023, 52, 6)));
    // Leap year starting on a Sunday needs a 54th column.
    assert_eq!(calendar_cell(days(2012, 12, 31)), Some((2012, 53, 0)));
}

#[test]
fn a_calendar_folds_each_day_and_needs_dates() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "when".into(),
            data_type: "Timestamp(Microsecond, None)".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::DateTime("2024-01-01 08:00:00".into()),
            CellValue::Int(2),
        ],
        vec![
            CellValue::DateTime("2024-01-01 20:00:00".into()),
            CellValue::Int(3),
        ],
        vec![
            CellValue::DateTime("2024-01-02 09:00:00".into()),
            CellValue::Int(4),
        ],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Calendar,
        x_col: Some(0),
        y_cols: vec![1],
        agg: Aggregation::Sum,
        ..ChartConfig::default()
    };
    let prep = build_chart(&t, &[0, 1, 2], &cfg, ChartLimits::default()).unwrap();
    assert_eq!(
        prep.data,
        ChartData::Calendar {
            days: vec![(days(2024, 1, 1), 5.0), (days(2024, 1, 2), 4.0)]
        }
    );
    let cfg = ChartConfig {
        x_col: Some(1),
        ..cfg
    };
    assert!(matches!(
        build_chart(&t, &[0, 1, 2], &cfg, ChartLimits::default()),
        Err(ChartError::XNotDate { .. })
    ));
}

#[test]
fn the_heat_scale_is_sequential_or_diverging_around_zero() {
    assert_eq!(heat_rgb(0.0, 0.0, 10.0), [0xde, 0xeb, 0xf7]);
    assert_eq!(heat_rgb(10.0, 0.0, 10.0), [0x08, 0x45, 0x94]);
    assert_eq!(heat_rgb(0.0, -5.0, 10.0), [0xf7, 0xf7, 0xf7]);
    assert_eq!(heat_rgb(-10.0, -10.0, 10.0), [0x21, 0x66, 0xac]);
    assert_eq!(text_on([0x08, 0x45, 0x94]), [0xff, 0xff, 0xff]);
    assert_eq!(text_on([0xf7, 0xf7, 0xf7]), [0x22, 0x22, 0x22]);
}

fn bars(values: &[(&str, f64)]) -> ChartData {
    ChartData::Bars {
        categories: values.iter().map(|(c, _)| c.to_string()).collect(),
        series: vec![
            ChartSeries {
                name: "revenue".into(),
                points: values
                    .iter()
                    .enumerate()
                    .map(|(i, (_, v))| [i as f64, *v])
                    .collect(),
            },
            ChartSeries {
                name: "second".into(),
                points: vec![[0.0, 1.0]],
            },
        ],
    }
}

#[test]
fn a_pareto_chart_sorts_the_first_series_and_runs_the_share_to_100() {
    let mut data = bars(&[("c", 10.0), ("a", 60.0), ("b", 30.0)]);
    pareto_sort(&mut data);
    let ChartData::Bars { categories, series } = &data else {
        panic!("still bars");
    };
    assert_eq!(categories, &["a", "b", "c"]);
    assert_eq!(series.len(), 1, "only the first series is a Pareto chart");
    assert_eq!(
        series[0].points,
        vec![[0.0, 60.0], [1.0, 30.0], [2.0, 10.0]]
    );
    assert_eq!(pareto_share(&data), Some(vec![60.0, 90.0, 100.0]));
}

#[test]
fn a_share_of_a_total_needs_bars_of_zero_or_more() {
    assert_eq!(pareto_share(&bars(&[("a", 5.0), ("b", -1.0)])), None);
    assert_eq!(pareto_share(&bars(&[("a", 0.0)])), None);
    assert_eq!(pareto_share(&ChartData::Boxes(Vec::new())), None);
}
