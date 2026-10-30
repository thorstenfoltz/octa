use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn cols() -> Vec<ColumnInfo> {
    ["ts", "amount", "region"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Utf8".into(),
        })
        .to_vec()
}

#[test]
fn resample_buckets_and_aggregates() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Month,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    let sql = build_resample_sql(&spec, &cols()).unwrap();
    assert_eq!(
        sql,
        "SELECT date_trunc('month', TRY_CAST(\"ts\" AS TIMESTAMP)) AS \"bucket\", \
sum(\"amount\") AS \"amount\" FROM data \
GROUP BY date_trunc('month', TRY_CAST(\"ts\" AS TIMESTAMP)) ORDER BY \"bucket\""
    );
}

#[test]
fn resample_groups_by_extra_columns() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Day,
        agg: TimeAgg::Mean,
        group_by: vec!["region".into()],
    };
    let sql = build_resample_sql(&spec, &cols()).unwrap();
    assert!(sql.contains("\"region\""), "{sql}");
    assert!(sql.contains("avg(\"amount\")"), "{sql}");
    assert!(sql.ends_with("ORDER BY \"bucket\", \"region\""), "{sql}");
}

#[test]
fn the_bucket_column_dodges_a_name_collision() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Day,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    let existing = named(&["ts", "amount", "bucket"]);
    let sql = build_resample_sql(&spec, &existing).unwrap();
    assert!(sql.contains("AS \"bucket_2\""), "{sql}");
}

#[test]
fn rolling_emits_an_ordered_window_frame() {
    let spec = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 7,
        agg: TimeAgg::Mean,
        partition_by: Vec::new(),
    };
    let sql = build_rolling_sql(&spec, &cols()).unwrap();
    assert_eq!(
        sql,
        "SELECT *, avg(\"amount\") OVER (ORDER BY \"ts\" \
ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) AS \"amount_rolling_7\" FROM data ORDER BY \"ts\""
    );
}

#[test]
fn rolling_partitions_when_asked() {
    let spec = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 3,
        agg: TimeAgg::Sum,
        partition_by: vec!["region".into()],
    };
    let sql = build_rolling_sql(&spec, &cols()).unwrap();
    assert!(
        sql.contains("PARTITION BY \"region\" ORDER BY \"ts\""),
        "{sql}"
    );
}

#[test]
fn a_window_of_one_is_the_current_row_only() {
    let spec = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 1,
        agg: TimeAgg::Sum,
        partition_by: Vec::new(),
    };
    let sql = build_rolling_sql(&spec, &cols()).unwrap();
    assert!(
        sql.contains("ROWS BETWEEN 0 PRECEDING AND CURRENT ROW"),
        "{sql}"
    );
}

#[test]
fn identifiers_with_quotes_are_escaped() {
    let spec = ResampleSpec {
        time_col: "we\"ird".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Year,
        agg: TimeAgg::Max,
        group_by: Vec::new(),
    };
    let sql = build_resample_sql(&spec, &named(&["we\"ird", "amount"])).unwrap();
    assert!(sql.contains("\"we\"\"ird\""), "{sql}");
}

#[test]
fn unknown_columns_are_rejected() {
    let spec = ResampleSpec {
        time_col: "nope".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Day,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    assert!(matches!(
        build_resample_sql(&spec, &cols()),
        Err(TimeseriesError::UnknownColumn(_))
    ));
}

#[test]
fn resample_needs_at_least_one_value_column() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: Vec::new(),
        interval: Interval::Day,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    assert!(matches!(
        build_resample_sql(&spec, &cols()),
        Err(TimeseriesError::NoValueColumns)
    ));
}

#[test]
fn a_zero_window_is_rejected() {
    let spec = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 0,
        agg: TimeAgg::Sum,
        partition_by: Vec::new(),
    };
    assert!(matches!(
        build_rolling_sql(&spec, &cols()),
        Err(TimeseriesError::ZeroWindow)
    ));
}

#[test]
fn explanations_name_the_columns_and_the_interval() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Week,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    let text = explain_resample(&spec);
    assert!(text.contains("ts"), "{text}");
    assert!(text.contains("amount"), "{text}");
    assert!(text.contains("week"), "{text}");

    let roll = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 7,
        agg: TimeAgg::Mean,
        partition_by: Vec::new(),
    };
    let text = explain_rolling(&roll);
    assert!(text.contains('7'), "{text}");
    assert!(text.contains("amount"), "{text}");
}

#[test]
fn duckdb_accepts_the_generated_sql() {
    // The string assertions above pin the shape; this pins that DuckDB will
    // actually run it, which no amount of string matching proves.
    use crate::data::{CellValue, ColumnInfo, DataTable};

    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "ts".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("2024-01-05".into()), CellValue::Int(10)],
        vec![CellValue::String("2024-01-20".into()), CellValue::Int(5)],
        vec![CellValue::String("2024-02-02".into()), CellValue::Int(7)],
    ];
    let cols = t.columns.clone();

    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Month,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    let sql = build_resample_sql(&spec, &cols).unwrap();
    let out = crate::sql::run_query(&t, &sql).expect("DuckDB should accept the resample SQL");
    assert_eq!(out.table.row_count(), 2, "January and February");

    let roll = RollingSpec {
        order_col: "ts".into(),
        value_col: "amount".into(),
        window: 2,
        agg: TimeAgg::Sum,
        partition_by: Vec::new(),
    };
    let sql = build_rolling_sql(&roll, &cols).unwrap();
    let out = crate::sql::run_query(&t, &sql).expect("DuckDB should accept the rolling SQL");
    assert_eq!(out.table.row_count(), 3, "a window keeps every row");
}

#[test]
fn interval_and_agg_parse_from_cli_words() {
    assert_eq!(Interval::parse("Month"), Some(Interval::Month));
    assert_eq!(Interval::parse("quarter"), Some(Interval::Quarter));
    assert_eq!(Interval::parse("fortnight"), None);
    assert_eq!(TimeAgg::parse("mean"), Some(TimeAgg::Mean));
    assert_eq!(TimeAgg::parse("AVG"), Some(TimeAgg::Mean));
    assert_eq!(TimeAgg::parse("median"), None);
}

#[test]
fn first_and_last_are_by_time_not_by_row_order() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    // Not in time order: the earliest January row is the second one, and
    // the January 10th row has no value (skipped, as on a database).
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "ts".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("2024-01-20".into()), CellValue::Int(1)],
        vec![CellValue::String("2024-01-05".into()), CellValue::Int(2)],
        vec![CellValue::String("2024-01-10".into()), CellValue::Null],
    ];
    let cols = t.columns.clone();
    let run = |agg| {
        let spec = ResampleSpec {
            time_col: "ts".into(),
            value_cols: vec!["amount".into()],
            interval: Interval::Month,
            agg,
            group_by: Vec::new(),
        };
        let sql = build_resample_sql(&spec, &cols).unwrap();
        crate::sql::run_query(&t, &sql)
            .unwrap()
            .table
            .get(0, 1)
            .unwrap()
            .to_string()
    };
    assert_eq!(run(TimeAgg::First), "2", "the January 5th value");
    assert_eq!(run(TimeAgg::Last), "1", "the January 20th value");
}

#[test]
fn first_is_spelled_with_arg_min() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["amount".into()],
        interval: Interval::Day,
        agg: TimeAgg::First,
        group_by: Vec::new(),
    };
    let sql = build_resample_sql(&spec, &cols()).unwrap();
    assert!(
        sql.contains("arg_min(\"amount\", TRY_CAST(\"ts\" AS TIMESTAMP)) AS \"amount\""),
        "{sql}"
    );
    assert!(explain_resample(&spec).contains("earliest"));
}

fn named(names: &[&str]) -> Vec<ColumnInfo> {
    names
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".into(),
        })
        .collect()
}

/// A Duration value column sums per bucket and over a window, to Durations
/// (DuckDB has no `sum` over INTERVAL). A clock time offers no sum or mean.
#[test]
fn a_duration_column_resamples_and_rolls_to_durations() {
    let h = 3_600_000_000_000i64;
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "ts".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "worked".into(),
            data_type: "Duration(µs)".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::String("2024-01-05".into()),
            CellValue::Duration(8 * h),
        ],
        vec![
            CellValue::String("2024-01-20".into()),
            CellValue::Duration(h / 2),
        ],
        vec![
            CellValue::String("2024-02-02".into()),
            CellValue::Duration(2 * h),
        ],
    ];
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["worked".into()],
        interval: Interval::Month,
        agg: TimeAgg::Sum,
        group_by: Vec::new(),
    };
    let sql = build_resample_sql(&spec, &t.columns).unwrap();
    let out = crate::sql::run_query(&t, &sql)
        .expect("DuckDB runs the resample")
        .table;
    assert_eq!(out.get(0, 1), Some(&CellValue::Duration(8 * h + h / 2)));
    let roll = RollingSpec {
        order_col: "ts".into(),
        value_col: "worked".into(),
        window: 2,
        agg: TimeAgg::Sum,
        partition_by: Vec::new(),
    };
    let sql = build_rolling_sql(&roll, &t.columns).unwrap();
    let out = crate::sql::run_query(&t, &sql)
        .expect("DuckDB runs the window")
        .table;
    assert_eq!(out.get(2, 2), Some(&CellValue::Duration(2 * h + h / 2)));
    assert!(!TimeAgg::for_type("Time64(µs)").contains(&TimeAgg::Mean));
    assert!(TimeAgg::for_type("Time64(µs)").contains(&TimeAgg::Last));
    assert_eq!(TimeAgg::for_type("Duration(µs)"), TimeAgg::ALL);
}
