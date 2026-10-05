//! Time series on the server, in the shape the file path's DuckDB SQL gives
//! (`crate::data::timeseries`): Resample buckets a typed time column
//! (`dialect::date_bucket`); First / Last are the value at the earliest /
//! latest time in the bucket, as DuckDB's `arg_min` / `arg_max`.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use crate::data::timeseries::{ResampleSpec, RollingSpec, TimeAgg, bucket_name};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{as_float, date_bucket, subquery_alias};
use super::lookups::{cap_clause, cut};
use super::{ServerSource, check_cancel};

/// A bucket's aggregate; `None` for First / Last, which need the row at the
/// earliest / latest time ([`at_time`]). Sum and Mean cast first: SQL
/// Server's SUM(int) overflows and its AVG(int) truncates.
fn plain_agg(e: DbEngine, agg: TimeAgg, v: &str) -> Option<String> {
    Some(match agg {
        TimeAgg::Sum => format!("SUM({})", as_float(e, v)),
        TimeAgg::Mean => format!("AVG({})", as_float(e, v)),
        TimeAgg::Min => format!("MIN({v})"),
        TimeAgg::Max => format!("MAX({v})"),
        TimeAgg::Count => format!("COUNT({v})"),
        TimeAgg::First | TimeAgg::Last => return None,
    })
}

/// The bucket and grouping cells as one map key; `Debug` keeps NULL and the
/// empty string apart.
fn key(cells: &[CellValue]) -> String {
    format!("{cells:?}")
}

/// `{bucket} AS octa_b, {g0} AS octa_g0, ...` for `part` = bucket, groups.
fn key_select(part: &[String]) -> String {
    part.iter()
        .enumerate()
        .map(|(i, x)| match i {
            0 => format!("{x} AS octa_b"),
            _ => format!("{x} AS octa_g{}", i - 1),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Value `v` at the earliest (`first`) or latest time of each bucket and
/// group (`part`: the bucket expression, then the grouping columns), among
/// the rows that have both a time and a value (DuckDB's `arg_min` /
/// `arg_max` skip the others), keyed by [`key`]; and the value's type. Ties
/// at one time pick any of the tied rows. `tail` is the main query's ORDER
/// BY and cap, so a cut result still gets every value it shows: the
/// connector stops reading at its own cap.
fn at_time(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    part: &[String],
    t: &str,
    v: &str,
    first: bool,
    tail: &str,
) -> anyhow::Result<(HashMap<String, CellValue>, String)> {
    let e = src.engine();
    let dir = if first { "ASC" } else { "DESC" };
    let r = c.query(&format!(
        "SELECT * FROM (SELECT {}, {v} AS octa_v, ROW_NUMBER() OVER (PARTITION BY {} ORDER BY {t} {dir}) AS octa_rn \
         FROM {} WHERE {t} IS NOT NULL AND {v} IS NOT NULL){} WHERE octa_rn = 1{tail}",
        key_select(part),
        part.join(", "),
        src.from_sql(),
        subquery_alias(e, "x")
    ))?;
    let k = part.len();
    let ty = r
        .columns
        .get(k)
        .map_or_else(|| "Utf8".to_string(), |c| c.data_type.clone());
    let map = r
        .rows
        .iter()
        .map(|row| (key(&row[..k]), row[k].clone()))
        .collect();
    Ok((map, ty))
}

/// Resample `src`: one row per bucket of `spec.time_col` (a typed column)
/// and combination of `spec.group_by`, ordered by bucket (empty bucket
/// last) then group, with `spec.agg` of each value column, named as the
/// file path names them. At most `cap` rows; the flag says the database has
/// more.
pub fn resample(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    columns: &[String],
    spec: &ResampleSpec,
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, bool)> {
    let e = src.engine();
    let q = |n: &str| e.quote_ident(n);
    let t = q(&spec.time_col);
    let b = date_bucket(e, &t, spec.interval);
    let g: Vec<String> = spec.group_by.iter().map(|x| q(x)).collect();
    let by_time = matches!(spec.agg, TimeAgg::First | TimeAgg::Last);
    let values: Vec<String> = if by_time {
        vec!["COUNT(*) AS octa_n".into()]
    } else {
        spec.value_cols
            .iter()
            .enumerate()
            .filter_map(|(i, v)| plain_agg(e, spec.agg, &q(v)).map(|a| format!("{a} AS octa_v{i}")))
            .collect()
    };
    let part: Vec<String> = std::iter::once(b.clone())
        .chain(g.iter().cloned())
        .collect();
    let order: Vec<String> =
        std::iter::once("CASE WHEN octa_b IS NULL THEN 1 ELSE 0 END, octa_b".to_string())
            .chain((0..g.len()).map(|i| format!("octa_g{i}")))
            .collect();
    let tail = format!(" ORDER BY {}{}", order.join(", "), cap_clause(e, cap));
    check_cancel(cancel)?;
    let main = c.query(&format!(
        "SELECT * FROM (SELECT {}, {} FROM {} GROUP BY {}){}{tail}",
        key_select(&part),
        values.join(", "),
        src.from_sql(),
        part.join(", "),
        subquery_alias(e, "x"),
    ))?;
    let k = 1 + g.len();
    let mut rows: Vec<Vec<CellValue>> = main.rows.iter().map(|r| r[..k].to_vec()).collect();
    let mut value_types: Vec<String> = Vec::new();
    if by_time {
        for v in &spec.value_cols {
            check_cancel(cancel)?;
            let first = spec.agg == TimeAgg::First;
            let (at, ty) = at_time(c, src, &part, &t, &q(v), first, &tail)?;
            value_types.push(ty);
            for row in &mut rows {
                let cell = at.get(&key(&row[..k])).cloned().unwrap_or(CellValue::Null);
                row.push(cell);
            }
        }
    } else {
        value_types = main.columns[k..]
            .iter()
            .map(|c| c.data_type.clone())
            .collect();
        for (row, r) in rows.iter_mut().zip(&main.rows) {
            row.extend(r[k..].iter().cloned());
        }
    }
    let names = std::iter::once(bucket_name(columns))
        .chain(spec.group_by.iter().cloned())
        .chain(spec.value_cols.iter().cloned());
    let types = main.columns[..k]
        .iter()
        .map(|c| c.data_type.clone())
        .chain(value_types);
    let mut out = DataTable::empty();
    out.columns = names
        .zip(types)
        .map(|(name, data_type)| ColumnInfo { name, data_type })
        .collect();
    out.rows = rows;
    Ok(cut(out, cap))
}

/// Whether `engine` keeps an empty value in a First / Last window, as DuckDB
/// (the file path) does: ClickHouse's `first_value` / `last_value` skip
/// NULL, so there the loaded rows are the only offer.
pub fn rolling_supported(engine: DbEngine, agg: TimeAgg) -> bool {
    !(engine == DbEngine::ClickHouse && matches!(agg, TimeAgg::First | TimeAgg::Last))
}

/// Every row of `src` with `spec.agg` over the last `spec.window` rows in
/// time order appended, ordered by time (rows without a time last, as in
/// DuckDB) then `tie`. At most `cap` rows; the flag says the database has
/// more.
pub fn rolling(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    spec: &RollingSpec,
    tie: &[String],
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, bool)> {
    let e = src.engine();
    let q = |n: &str| e.quote_ident(n);
    let v = q(&spec.value_col);
    let t = q(&spec.order_col);
    let f = match spec.agg {
        TimeAgg::Sum => format!("SUM({})", as_float(e, &v)),
        TimeAgg::Mean => format!("AVG({})", as_float(e, &v)),
        TimeAgg::Min => format!("MIN({v})"),
        TimeAgg::Max => format!("MAX({v})"),
        TimeAgg::Count => format!("COUNT({v})"),
        TimeAgg::First => format!("FIRST_VALUE({v})"),
        TimeAgg::Last => format!("LAST_VALUE({v})"),
    };
    // The key orders rows of equal time, in the frame and in the result
    // alike, so a window over a tie is the same on every run.
    let order: Vec<String> =
        std::iter::once(format!("CASE WHEN {t} IS NULL THEN 1 ELSE 0 END, {t}"))
            .chain(tie.iter().map(|k| q(k)))
            .collect();
    let mut frame = String::new();
    if !spec.partition_by.is_empty() {
        let parts: Vec<String> = spec.partition_by.iter().map(|p| q(p)).collect();
        frame.push_str(&format!("PARTITION BY {} ", parts.join(", ")));
    }
    frame.push_str(&format!(
        "ORDER BY {} ROWS BETWEEN {} PRECEDING AND CURRENT ROW",
        order.join(", "),
        spec.window.saturating_sub(1)
    ));
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT x.*, {f} OVER ({frame}) FROM {} ORDER BY {}{}",
        src.from_as("x"),
        order.join(", "),
        cap_clause(e, cap)
    ))?;
    let (mut t, more) = cut(t, cap);
    if let Some(last) = t.columns.last_mut() {
        last.name = format!("{}_rolling_{}", spec.value_col, spec.window);
    }
    Ok((t, more))
}

#[cfg(test)]
pub(crate) mod tests {
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
}
