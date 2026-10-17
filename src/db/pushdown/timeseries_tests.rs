use super::super::test_support::{DuckConn, source};
use super::*;
use crate::data::timeseries::{Interval, build_resample_sql};

/// Not in time order; a row without a value; two regions; a row
/// without a time (its own empty bucket, as in DuckDB).
pub(crate) fn table() -> DataTable {
    let row = |ts: Option<&str>, n: Option<i64>, r: &str| {
        vec![
            ts.map_or(CellValue::Null, |s| CellValue::DateTime(s.into())),
            n.map_or(CellValue::Null, CellValue::Int),
            CellValue::String(r.into()),
        ]
    };
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "ts".into(),
            data_type: "Timestamp(Microsecond, None)".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "region".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = vec![
        row(Some("2024-01-20 08:00:00"), Some(1), "north"),
        row(Some("2024-01-05 09:00:00"), Some(2), "north"),
        row(Some("2024-01-10 10:00:00"), None, "north"),
        row(Some("2024-01-07 11:00:00"), Some(4), "south"),
        row(Some("2024-02-02 12:00:00"), Some(5), "north"),
        row(None, Some(6), "south"),
    ];
    t
}

fn cols() -> Vec<String> {
    vec!["ts".into(), "n".into(), "region".into()]
}

pub(crate) fn norm(t: &DataTable) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = t
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| match super::super::cell_f64(c) {
                    Some(x) => format!("{x}"),
                    None => c.to_string(),
                })
                .collect()
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn the_server_resample_is_the_file_resample() {
    for &agg in TimeAgg::ALL {
        for (interval, group) in [
            (Interval::Month, vec![]),
            (Interval::Week, vec!["region".to_string()]),
        ] {
            let spec = ResampleSpec {
                time_col: "ts".into(),
                value_cols: vec!["n".into()],
                interval,
                agg,
                group_by: group,
            };
            let want =
                crate::sql::run_query(&table(), &build_resample_sql(&spec, &cols()).unwrap())
                    .unwrap()
                    .table;
            let mut c = DuckConn::new(table());
            let (got, more) = resample(
                &mut c,
                &source(),
                &cols(),
                &spec,
                1_000,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(!more);
            let names =
                |t: &DataTable| t.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
            assert_eq!(names(&got), names(&want), "{agg:?} {interval:?}");
            assert_eq!(norm(&got), norm(&want), "{agg:?} {interval:?}");
        }
    }
}

#[test]
fn more_buckets_than_the_cap_says_so() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["n".into()],
        interval: Interval::Day,
        agg: TimeAgg::Sum,
        group_by: vec![],
    };
    let mut c = DuckConn::new(table());
    let (got, more) = resample(
        &mut c,
        &source(),
        &cols(),
        &spec,
        2,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(more);
    assert_eq!(got.row_count(), 2);
}

#[test]
fn the_server_rolling_window_is_the_file_one() {
    use crate::data::timeseries::{RollingSpec, build_rolling_sql};
    // No two rows share a time: the window order is then the same on
    // both sides. The row without a time is left out of both.
    let mut t = table();
    t.rows.pop();
    for &agg in TimeAgg::ALL {
        for partition in [vec![], vec!["region".to_string()]] {
            let spec = RollingSpec {
                order_col: "ts".into(),
                value_col: "n".into(),
                window: 2,
                agg,
                partition_by: partition,
            };
            let want = crate::sql::run_query(&t, &build_rolling_sql(&spec, &cols()).unwrap())
                .unwrap()
                .table;
            let mut c = DuckConn::new(t.clone());
            let (got, more) = rolling(
                &mut c,
                &source(),
                &spec,
                &[],
                1_000,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(!more);
            assert_eq!(
                got.columns.last().unwrap().name,
                want.columns.last().unwrap().name
            );
            assert_eq!(norm(&got), norm(&want), "{agg:?}");
        }
    }
}

#[test]
fn clickhouse_cannot_keep_nulls_in_a_first_window() {
    assert!(!rolling_supported(DbEngine::ClickHouse, TimeAgg::First));
    assert!(rolling_supported(DbEngine::ClickHouse, TimeAgg::Sum));
    assert!(rolling_supported(DbEngine::Postgres, TimeAgg::Last));
}

/// A cut result: the First statement reads the same first buckets as the
/// main one (same ORDER BY and cap), so the connector's own cap cannot
/// drop a value the result shows. Rolling orders ties by the key inside
/// the frame too.
#[test]
fn a_cut_first_reads_the_same_buckets_and_ties_follow_the_key() {
    let spec = ResampleSpec {
        time_col: "ts".into(),
        value_cols: vec!["n".into()],
        interval: Interval::Day,
        agg: TimeAgg::First,
        group_by: vec![],
    };
    let mut c = DuckConn::new(table());
    let (got, more) = resample(
        &mut c,
        &source(),
        &cols(),
        &spec,
        2,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(more);
    assert_eq!(
        norm(&got),
        vec![
            vec!["2024-01-05 00:00:00", "2"],
            vec!["2024-01-07 00:00:00", "4"]
        ]
    );
    let first = c.log.last().unwrap();
    assert!(
        first.contains("octa_rn = 1 ORDER BY") && first.ends_with(" LIMIT 3"),
        "{first}"
    );

    use crate::data::timeseries::RollingSpec;
    let spec = RollingSpec {
        order_col: "ts".into(),
        value_col: "n".into(),
        window: 2,
        agg: TimeAgg::Sum,
        partition_by: vec![],
    };
    let mut c = DuckConn::new(table());
    rolling(
        &mut c,
        &source(),
        &spec,
        &["region".into()],
        100,
        &AtomicBool::new(false),
    )
    .unwrap();
    let sql = c.log.last().unwrap();
    assert!(sql.contains("\"ts\", \"region\" ROWS BETWEEN"), "{sql}");
}
