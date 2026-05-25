//! Integration tests for the chart data-prep pipeline (F1).
//!
//! These tests poke at `octa::data::chart::build_chart` directly — the
//! renderer side (egui_plot) is GUI-driven and tested manually. The point
//! here is that sampling, aggregation, kind→column gating, and the bad-
//! column error paths all behave predictably.

use octa::data::CellValue;
use octa::data::ColumnInfo;
use octa::data::DataTable;
use octa::data::chart::{
    Aggregation, ChartConfig, ChartData, ChartError, ChartKind, ChartLimits,
    DEFAULT_MAX_BAR_CATEGORIES, XAxisKind, build_chart, format_days_as_date,
    format_seconds_as_datetime, has_numeric_column,
};

fn limits() -> ChartLimits {
    ChartLimits {
        max_points: 100_000,
        max_categories: DEFAULT_MAX_BAR_CATEGORIES,
    }
}

fn no_sampling() -> ChartLimits {
    ChartLimits {
        max_points: 0,
        max_categories: DEFAULT_MAX_BAR_CATEGORIES,
    }
}

fn limits_with_categories(max_categories: usize) -> ChartLimits {
    ChartLimits {
        max_points: 100_000,
        max_categories,
    }
}

fn numeric_table(n: usize) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "y".into(),
            data_type: "Float64".into(),
        },
    ];
    t.rows = (0..n)
        .map(|i| vec![CellValue::Int(i as i64), CellValue::Float((i as f64) * 2.0)])
        .collect();
    t
}

fn cat_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "fruit".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "qty".into(),
            data_type: "Int64".into(),
        },
    ];
    let rows: Vec<(&str, i64)> = vec![
        ("apple", 3),
        ("apple", 7),
        ("pear", 2),
        ("pear", 4),
        ("plum", 9),
    ];
    t.rows = rows
        .into_iter()
        .map(|(f, q)| vec![CellValue::String(f.into()), CellValue::Int(q)])
        .collect();
    t
}

fn all_rows(t: &DataTable) -> Vec<usize> {
    (0..t.row_count()).collect()
}

#[test]
fn chart_sampling_caps_at_max_points() {
    let t = numeric_table(200_000);
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    assert_eq!(prep.total_rows, 200_000);
    assert_eq!(prep.used_rows, 100_000);
    if let ChartData::Scatter { series, .. } = prep.data {
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].points.len(), 100_000);
    } else {
        panic!("expected scatter data");
    }
}

#[test]
fn chart_disables_sampling_when_max_points_is_zero() {
    let t = numeric_table(2_500);
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, no_sampling()).unwrap();
    assert_eq!(prep.used_rows, prep.total_rows);
    if let ChartData::Lines { series, .. } = prep.data {
        assert_eq!(series[0].points.len(), 2_500);
    } else {
        panic!("expected line data");
    }
}

#[test]
fn histogram_uses_sturges_when_bins_unset() {
    let t = numeric_table(64);
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(0),
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    if let ChartData::Histogram { bins, .. } = prep.data {
        // ceil(1 + log2(64)) = 7
        assert_eq!(bins.len(), 7);
        let total: f64 = bins.iter().map(|b| b.1).sum();
        assert_eq!(total as usize, 64);
    } else {
        panic!("expected histogram");
    }
}

#[test]
fn histogram_respects_explicit_bin_count() {
    let t = numeric_table(100);
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(0),
        hist_bins: Some(10),
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    if let ChartData::Histogram { bins, .. } = prep.data {
        assert_eq!(bins.len(), 10);
    } else {
        panic!();
    }
}

#[test]
fn bar_aggregates_by_category() {
    let t = cat_table();
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![1],
        agg: Aggregation::Sum,
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    if let ChartData::Bars { categories, series } = prep.data {
        assert_eq!(categories, vec!["apple", "pear", "plum"]);
        let expected = [10.0, 6.0, 9.0];
        assert_eq!(series.len(), 1);
        let actual: Vec<f64> = series[0].points.iter().map(|p| p[1]).collect();
        assert_eq!(actual, expected);
    } else {
        panic!("expected bars");
    }
}

#[test]
fn bar_avg_count_min_max_fold_correctly() {
    let t = cat_table();
    let cases = [
        (Aggregation::Avg, [5.0, 3.0, 9.0]),
        (Aggregation::Count, [2.0, 2.0, 1.0]),
        (Aggregation::Min, [3.0, 2.0, 9.0]),
        (Aggregation::Max, [7.0, 4.0, 9.0]),
    ];
    for (agg, expected) in cases {
        let cfg = ChartConfig {
            kind: ChartKind::Bar,
            x_col: Some(0),
            y_cols: vec![1],
            agg,
            ..Default::default()
        };
        let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
        if let ChartData::Bars { series, .. } = prep.data {
            let actual: Vec<f64> = series[0].points.iter().map(|p| p[1]).collect();
            assert_eq!(actual, expected, "agg={:?}", agg);
        } else {
            panic!();
        }
    }
}

#[test]
fn bar_rejects_too_many_categories() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        },
    ];
    let n = DEFAULT_MAX_BAR_CATEGORIES + 5;
    t.rows = (0..n)
        .map(|i| vec![CellValue::String(format!("k{i}")), CellValue::Int(1)])
        .collect();
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![1],
        agg: Aggregation::Sum,
        ..Default::default()
    };
    let err = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap_err();
    assert!(
        matches!(err, ChartError::TooManyCategories { .. }),
        "{err:?}"
    );
}

#[test]
fn line_sorts_by_x() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "y".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Int(5), CellValue::Int(50)],
        vec![CellValue::Int(1), CellValue::Int(10)],
        vec![CellValue::Int(3), CellValue::Int(30)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    if let ChartData::Lines { series, .. } = prep.data {
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![1.0, 3.0, 5.0]);
    } else {
        panic!();
    }
}

#[test]
fn box_plot_summary_matches_known_values() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "v".into(),
        data_type: "Float64".into(),
    }];
    t.rows = (1..=9).map(|i| vec![CellValue::Float(i as f64)]).collect();
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        x_col: None,
        y_cols: vec![0],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    if let ChartData::Boxes(b) = prep.data {
        let s = &b[0];
        assert!((s.median - 5.0).abs() < 1e-9);
        assert!((s.q1 - 3.0).abs() < 1e-9);
        assert!((s.q3 - 7.0).abs() < 1e-9);
        assert_eq!(s.lower_whisker, 1.0);
        assert_eq!(s.upper_whisker, 9.0);
    } else {
        panic!();
    }
}

#[test]
fn missing_x_returns_no_x_column_error() {
    let t = numeric_table(10);
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: None,
        ..Default::default()
    };
    assert_eq!(
        build_chart(&t, &all_rows(&t), &cfg, limits()),
        Err(ChartError::NoXColumn)
    );
}

#[test]
fn missing_y_returns_no_y_column_for_kinds_that_need_it() {
    let t = numeric_table(10);
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![],
        ..Default::default()
    };
    assert_eq!(
        build_chart(&t, &all_rows(&t), &cfg, limits()),
        Err(ChartError::NoYColumn)
    );
}

#[test]
fn empty_filter_returns_empty_after_filter() {
    let t = numeric_table(10);
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(0),
        ..Default::default()
    };
    assert_eq!(
        build_chart(&t, &[], &cfg, limits()),
        Err(ChartError::EmptyAfterFilter)
    );
}

#[test]
fn non_numeric_y_surfaces_y_not_numeric() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "label".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = (0..5)
        .map(|i| vec![CellValue::Int(i), CellValue::String(format!("row-{i}"))])
        .collect();
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let err = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap_err();
    assert!(matches!(err, ChartError::YNotNumeric { .. }), "{err:?}");
}

#[test]
fn has_numeric_column_detects_numeric_arrow_types() {
    let t = numeric_table(1);
    assert!(has_numeric_column(&t));

    let mut s = DataTable::empty();
    s.columns = vec![ColumnInfo {
        name: "only_text".into(),
        data_type: "Utf8".into(),
    }];
    assert!(!has_numeric_column(&s));
}

#[test]
fn filtered_rows_constrain_the_chart_input() {
    let t = numeric_table(20);
    // Only keep rows 0, 5, 10, 15 → expect 4 points.
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &[0, 5, 10, 15], &cfg, limits()).unwrap();
    assert_eq!(prep.total_rows, 4);
    assert_eq!(prep.used_rows, 4);
    if let ChartData::Scatter { series, .. } = prep.data {
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.0, 5.0, 10.0, 15.0]);
    } else {
        panic!();
    }
}

#[test]
fn chart_kind_label_and_all_are_consistent() {
    assert_eq!(ChartKind::ALL.len(), 5);
    for k in ChartKind::ALL {
        assert!(!k.label().is_empty());
    }
    for a in Aggregation::ALL {
        assert!(!a.label().is_empty());
    }
}

#[test]
fn chart_kind_needs_y_reflects_definition() {
    assert!(!ChartKind::Histogram.needs_y());
    assert!(ChartKind::Bar.needs_y());
    assert!(ChartKind::Line.needs_y());
    assert!(ChartKind::Scatter.needs_y());
    assert!(ChartKind::Box.needs_y());
}

#[test]
fn date_column_coerces_to_days_since_epoch() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "day".into(),
            data_type: "Date".into(),
        },
        ColumnInfo {
            name: "y".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Date("1970-01-01".into()), CellValue::Int(10)],
        vec![CellValue::Date("1970-01-02".into()), CellValue::Int(20)],
        vec![CellValue::Date("1970-01-11".into()), CellValue::Int(30)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..t.row_count()).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    if let ChartData::Scatter { series, .. } = prep.data {
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.0, 1.0, 10.0], "dates → days since epoch");
    } else {
        panic!();
    }
}

#[test]
fn datetime_column_coerces_to_seconds_since_epoch() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "ts".into(),
            data_type: "Timestamp".into(),
        },
        ColumnInfo {
            name: "y".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::DateTime("1970-01-01 00:00:00".into()),
            CellValue::Int(1),
        ],
        vec![
            CellValue::DateTime("1970-01-01 00:01:00".into()),
            CellValue::Int(2),
        ],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &[0, 1], &cfg, limits()).unwrap();
    if let ChartData::Scatter { series, .. } = prep.data {
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.0, 60.0]);
    } else {
        panic!();
    }
}

#[test]
fn settable_max_categories_overrides_default() {
    // A handful of distinct categories — well under 200 but over our
    // tightened cap of 3.
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = (0..5)
        .map(|i| vec![CellValue::String(format!("k{i}")), CellValue::Int(1)])
        .collect();
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![1],
        agg: Aggregation::Sum,
        ..Default::default()
    };
    let rows: Vec<usize> = (0..t.row_count()).collect();
    let err = build_chart(&t, &rows, &cfg, limits_with_categories(3)).unwrap_err();
    assert!(
        matches!(err, ChartError::TooManyCategories { cap: 3, .. }),
        "{err:?}"
    );
    // And bumping the cap above the input count makes the same call succeed.
    let prep = build_chart(&t, &rows, &cfg, limits_with_categories(10)).unwrap();
    if let ChartData::Bars { categories, .. } = prep.data {
        assert_eq!(categories.len(), 5);
    } else {
        panic!();
    }
}

#[test]
fn bar_categories_preserve_first_seen_order_for_x_axis_formatter() {
    // The chart view's x_axis_formatter looks up `categories[idx]`. If the
    // category order changes, the on-screen labels get out of sync with
    // the bars — guard against accidental reordering.
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "pop".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("DE".into()), CellValue::Int(83)],
        vec![CellValue::String("US".into()), CellValue::Int(333)],
        vec![CellValue::String("DE".into()), CellValue::Int(0)],
        vec![CellValue::String("JP".into()), CellValue::Int(125)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Bar,
        x_col: Some(0),
        y_cols: vec![1],
        agg: Aggregation::Sum,
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..4).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    if let ChartData::Bars { categories, .. } = prep.data {
        assert_eq!(categories, vec!["DE", "US", "JP"]);
    } else {
        panic!();
    }
}

#[test]
fn line_falls_back_to_categorical_when_x_is_non_numeric() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "fruit".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "qty".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("apple".into()), CellValue::Int(3)],
        vec![CellValue::String("pear".into()), CellValue::Int(7)],
        vec![CellValue::String("plum".into()), CellValue::Int(9)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..t.row_count()).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    if let ChartData::Lines {
        categories: Some(cats),
        series,
    } = prep.data
    {
        assert_eq!(cats, vec!["apple", "pear", "plum"]);
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        // Each row sits at its category index in first-seen order.
        assert_eq!(xs, vec![0.0, 1.0, 2.0]);
    } else {
        panic!("expected categorical Lines variant");
    }
}

#[test]
fn scatter_categorical_x_emits_categories() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "pop".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("DE".into()), CellValue::Int(83)],
        vec![CellValue::String("US".into()), CellValue::Int(333)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Scatter,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..2).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    if let ChartData::Scatter {
        categories: Some(cats),
        ..
    } = prep.data
    {
        assert_eq!(cats, vec!["DE", "US"]);
    } else {
        panic!("expected categorical Scatter variant");
    }
}

#[test]
fn x_axis_categories_returns_series_names_for_box() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "a".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "b".into(),
            data_type: "Float64".into(),
        },
    ];
    t.rows = (0..5)
        .map(|i| vec![CellValue::Float(i as f64), CellValue::Float(i as f64 * 2.0)])
        .collect();
    let cfg = ChartConfig {
        kind: ChartKind::Box,
        x_col: None,
        y_cols: vec![0, 1],
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..5).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    let categories = prep.data.x_axis_categories();
    assert_eq!(
        categories.as_deref(),
        Some(["a".to_string(), "b".to_string()].as_slice())
    );
}

#[test]
fn x_axis_categories_returns_none_for_numeric_line() {
    let t = numeric_table(10);
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    assert!(prep.data.x_axis_categories().is_none());
}

#[test]
fn line_categorical_with_too_many_categories_errors() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = (0..6)
        .map(|i| vec![CellValue::String(format!("k{i}")), CellValue::Int(1)])
        .collect();
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let err = build_chart(
        &t,
        &(0..t.row_count()).collect::<Vec<_>>(),
        &cfg,
        limits_with_categories(3),
    )
    .unwrap_err();
    assert!(
        matches!(err, ChartError::TooManyCategories { cap: 3, .. }),
        "{err:?}"
    );
}

#[test]
fn histogram_of_date_column_sets_x_axis_kind_date() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "day".into(),
        data_type: "Date".into(),
    }];
    t.rows = vec![
        vec![CellValue::Date("2024-01-01".into())],
        vec![CellValue::Date("2024-01-15".into())],
        vec![CellValue::Date("2024-02-01".into())],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(0),
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..3).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    assert_eq!(prep.x_axis_kind, XAxisKind::Date);
}

#[test]
fn histogram_of_datetime_column_sets_x_axis_kind_datetime() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "ts".into(),
        data_type: "Timestamp".into(),
    }];
    t.rows = vec![
        vec![CellValue::DateTime("2024-01-01 00:00:00".into())],
        vec![CellValue::DateTime("2024-01-01 12:00:00".into())],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Histogram,
        x_col: Some(0),
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..2).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    assert_eq!(prep.x_axis_kind, XAxisKind::DateTime);
}

#[test]
fn line_with_numeric_x_keeps_numeric_axis_kind() {
    let t = numeric_table(10);
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &all_rows(&t), &cfg, limits()).unwrap();
    assert_eq!(prep.x_axis_kind, XAxisKind::Numeric);
}

#[test]
fn line_with_date_x_sets_axis_kind_date() {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "day".into(),
            data_type: "Date".into(),
        },
        ColumnInfo {
            name: "y".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::Date("2024-01-01".into()), CellValue::Int(1)],
        vec![CellValue::Date("2024-01-08".into()), CellValue::Int(2)],
    ];
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    let prep = build_chart(&t, &(0..2).collect::<Vec<_>>(), &cfg, limits()).unwrap();
    assert_eq!(prep.x_axis_kind, XAxisKind::Date);
}

#[test]
fn format_days_as_date_roundtrips_unix_epoch() {
    assert_eq!(format_days_as_date(0.0), "1970-01-01");
    assert_eq!(format_days_as_date(1.0), "1970-01-02");
    // 2024-01-01 = 19723 days since the Unix epoch.
    assert_eq!(format_days_as_date(19_723.0), "2024-01-01");
}

#[test]
fn format_seconds_as_datetime_roundtrips_unix_epoch() {
    assert_eq!(format_seconds_as_datetime(0.0), "1970-01-01 00:00:00");
    assert_eq!(format_seconds_as_datetime(60.0), "1970-01-01 00:01:00");
}
