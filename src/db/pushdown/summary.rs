//! Summary on the server: one aggregate scan for every column, then a mode
//! query per column and a quartile query per numeric column when those
//! statistics are switched on. Columns and types come from the loaded tab,
//! which has the same header as the table.

use std::sync::atomic::AtomicBool;

use crate::data::summary::{
    ColumnStats, SummaryStat, active_stats, column_stats, num_cell, stats_table,
};
use crate::data::{CellValue, ColumnInfo, DataTable, is_numeric_data_type};
use crate::db::DbConnector;

use super::dialect::{
    as_float, distinct_key, exact, length_fn, limit_clause, present, quartiles_sql, stddev_fn, text,
};
use super::{LocalParts, ServerSource, cell_f64, cell_i64, check_cancel, per_column_aggregates};

/// Aggregate expressions for one column as two groups, so a server that
/// refuses a value aggregate (MIN on some type) still reports the null count.
/// `[0]` touches only the text form and works on any type: missing count,
/// min and max length. `[1]` is MIN, MAX, COUNT(DISTINCT) and, for numerics,
/// SUM, AVG, stddev. [`read_aggregates`] reads both by position.
fn aggregate_exprs(src: &ServerSource, col: &ColumnInfo) -> [Vec<String>; 2] {
    let e = src.engine();
    let q = e.quote_ident(&col.name);
    let len = format!("{}({})", length_fn(e), text(e, &q));
    let mut counts = vec![format!(
        "SUM(CASE WHEN {} THEN 0 ELSE 1 END)",
        present(e, &q)
    )];
    // A float's text is the database's own ("3"), not Octa's ("3.0"), and
    // engines differ: its length comes from the loaded rows instead.
    if !is_float(&col.data_type) {
        counts.push(format!("MIN({len})"));
        counts.push(format!("MAX({len})"));
    }
    // Postgres/Redshift have no min(boolean) and SQL Server rejects MIN on bit.
    let m = if col.data_type == "Boolean" {
        text(e, &q)
    } else {
        q.clone()
    };
    let mut values = vec![
        format!("MIN({m})"),
        format!("MAX({m})"),
        format!("COUNT(DISTINCT {})", distinct_key(e, &q, &col.data_type)),
    ];
    if is_numeric_data_type(&col.data_type) {
        let f = as_float(e, &q);
        // Cast first: SQL Server's SUM(int) overflows at 2^31, ClickHouse wraps.
        values.push(format!("SUM({f})"));
        values.push(format!("AVG({f})"));
        values.push(format!("{}({f})", stddev_fn(e)));
    }
    [counts, values]
}

fn is_float(data_type: &str) -> bool {
    data_type.starts_with("Float")
}

fn read_aggregates(
    counts: Option<&[CellValue]>,
    values: Option<&[CellValue]>,
    s: &mut ColumnStats,
) {
    let some =
        |c: &[CellValue], i: usize| c.get(i).filter(|v| !matches!(v, CellValue::Null)).cloned();
    if let Some(c) = counts {
        s.missing = c.first().and_then(cell_i64).unwrap_or(0);
        s.text_len_min = c.get(1).and_then(cell_i64);
        s.text_len_max = c.get(2).and_then(cell_i64);
    }
    if let Some(v) = values {
        s.min = some(v, 0);
        s.max = some(v, 1);
        s.unique = v.get(2).and_then(cell_i64);
        s.sum = v.get(3).and_then(cell_f64);
        s.mean = v.get(4).and_then(cell_f64).map(num_cell);
        s.std = v.get(5).and_then(cell_f64).map(num_cell);
    }
}

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    loaded: &DataTable,
    total: usize,
    enabled: &[SummaryStat],
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, LocalParts)> {
    let e = src.engine();
    let from = src.from_sql();
    let active = active_stats(enabled);
    let on = |s: SummaryStat| active.contains(&s);
    let mut stats: Vec<ColumnStats> = loaded
        .columns
        .iter()
        .map(|col| ColumnStats {
            name: col.name.clone(),
            type_name: col.data_type.clone(),
            ..Default::default()
        })
        .collect();

    // 2*N groups: column i's counts at 2i, its values at 2i + 1.
    let groups: Vec<Vec<String>> = loaded
        .columns
        .iter()
        .flat_map(|c| aggregate_exprs(src, c))
        .collect();
    let got = per_column_aggregates(c, &from, &groups, cancel)?;
    for (s, g) in stats.iter_mut().zip(got.chunks(2)) {
        read_aggregates(g[0].as_deref(), g.get(1).and_then(|v| v.as_deref()), s);
    }

    if on(SummaryStat::Mode) || on(SummaryStat::ModeCount) {
        for (s, col) in stats.iter_mut().zip(&loaded.columns) {
            check_cancel(cancel)?;
            let q = e.quote_ident(&col.name);
            let v = text(e, &q);
            // Ties broken on the value, as the in-memory pass does; grouped by
            // the exact text, as in memory (MySQL / SQL Server fold case).
            let sql = format!(
                "SELECT MIN({v}) AS octa_v, COUNT(*) AS octa_n FROM {from} WHERE {q} IS NOT NULL \
                 GROUP BY {} ORDER BY octa_n DESC, octa_v{}",
                exact(e, &v),
                limit_clause(e, 1)
            );
            match c.query(&sql) {
                Ok(t) => {
                    s.mode = t
                        .get(0, 0)
                        .filter(|v| !matches!(v, CellValue::Null))
                        .map(|v| v.to_string());
                    s.mode_count = t.get(0, 1).and_then(cell_i64);
                }
                // The cell stays blank; the other statistics still show.
                Err(e) => tracing::debug!("pushdown summary: mode of {} failed: {e:#}", col.name),
            }
        }
    }

    let mut local = LocalParts::default();
    let len_stats = [SummaryStat::TextLenMin, SummaryStat::TextLenMax];
    if len_stats.iter().any(|s| on(*s)) && loaded.columns.iter().any(|c| is_float(&c.data_type)) {
        let lens = column_stats(loaded, &len_stats)?;
        for ((s, col), l) in stats.iter_mut().zip(&loaded.columns).zip(&lens) {
            if is_float(&col.data_type) {
                s.text_len_min = l.text_len_min;
                s.text_len_max = l.text_len_max;
            }
        }
        local.by_design.push(format!(
            "{}, {} (Float)",
            SummaryStat::TextLenMin.column_id(),
            SummaryStat::TextLenMax.column_id()
        ));
    }
    let quartile_stats = [
        SummaryStat::Q25,
        SummaryStat::Median,
        SummaryStat::Q75,
        SummaryStat::Iqr,
    ];
    if quartile_stats.iter().any(|s| on(*s)) {
        let mut fallback: Option<Vec<ColumnStats>> = None;
        let (mut refused, mut gap) = (false, false);
        for (i, col) in loaded.columns.iter().enumerate() {
            if !is_numeric_data_type(&col.data_type) {
                continue;
            }
            check_cancel(cancel)?;
            let q = e.quote_ident(&col.name);
            let from_server = match quartiles_sql(e, &from, &q) {
                Some(sql) => match c.query(&sql) {
                    Ok(t) => {
                        let at = |k: usize| t.get(0, k).and_then(cell_f64).map(num_cell);
                        stats[i].q25 = at(0);
                        stats[i].median = at(1);
                        stats[i].q75 = at(2);
                        true
                    }
                    Err(e) => {
                        // A user Cancel must not read as "the server refused".
                        check_cancel(cancel)?;
                        tracing::debug!(
                            "pushdown summary: quartiles of {} failed: {e:#}",
                            col.name
                        );
                        refused = true;
                        false
                    }
                },
                // The engine has no percentile function.
                None => {
                    gap = true;
                    false
                }
            };
            if !from_server {
                // The loaded rows stand in, and the note says so.
                if fallback.is_none() {
                    fallback = Some(column_stats(loaded, &quartile_stats)?);
                }
                if let Some(l) = fallback.as_ref().and_then(|f| f.get(i)) {
                    stats[i].q25 = l.q25.clone();
                    stats[i].median = l.median.clone();
                    stats[i].q75 = l.q75.clone();
                }
            }
        }
        let named = || {
            quartile_stats
                .iter()
                .filter(|s| on(**s))
                .map(|s| s.column_id().to_string())
        };
        if gap {
            local.engine.extend(named());
        }
        if refused {
            local.failed.extend(named());
        }
    }

    Ok((stats_table(&stats, total, enabled), local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::summary::{SummaryStat, build_summary_table};
    use crate::data::{CellValue, ColumnInfo};
    use crate::db::pushdown::test_support::{DuckConn, source};
    use crate::db::{DbEngine, DbWriteMode, DbWriteReport};

    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "s".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = (0..40)
            .map(|i| {
                vec![
                    if i % 7 == 0 {
                        CellValue::Null
                    } else {
                        CellValue::Int(i % 9)
                    },
                    if i % 5 == 0 {
                        CellValue::String(String::new())
                    } else {
                        CellValue::String(format!("v{}", i % 3))
                    },
                ]
            })
            .collect();
        t
    }

    /// Stats both paths compute exactly. Quartiles are left out: SUMMARIZE's
    /// are approximate, the server's interpolate; the next test pins those.
    #[test]
    fn matches_the_in_memory_summary_on_the_same_rows() {
        let t = table();
        let exact = [
            SummaryStat::Min,
            SummaryStat::Max,
            SummaryStat::Sum,
            SummaryStat::Mean,
            SummaryStat::Std,
            SummaryStat::Mode,
            SummaryStat::ModeCount,
            SummaryStat::NotNullCount,
            SummaryStat::NullCount,
            SummaryStat::NullPercent,
            SummaryStat::UniqueCount,
            SummaryStat::DistinctRatio,
            SummaryStat::TextLenMin,
            SummaryStat::TextLenMax,
            SummaryStat::TotalRows,
        ];
        let local = build_summary_table(&t, &exact).unwrap();
        let mut c = DuckConn::new(t.clone());
        let (server, parts) = run(
            &mut c,
            &source(),
            &t,
            t.row_count(),
            &exact,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(parts.is_empty());
        assert_eq!(server.columns.len(), local.columns.len());
        // Column 1 is the type: SUMMARIZE says BIGINT, the server path says
        // the tab's own Int64. Every statistic column must agree.
        for r in 0..local.row_count() {
            for col in 2..local.col_count() {
                let (a, b) = (local.get(r, col).unwrap(), server.get(r, col).unwrap());
                match (cell_f64(a), cell_f64(b)) {
                    (Some(x), Some(y)) => assert!(
                        (x - y).abs() < 1e-9,
                        "r{r} {}: {x} vs {y}",
                        local.columns[col].name
                    ),
                    _ => assert_eq!(
                        a.to_string(),
                        b.to_string(),
                        "r{r} {}",
                        local.columns[col].name
                    ),
                }
            }
        }
    }

    /// A float's text length is Octa's form ("3.0"), whatever the server
    /// writes, and the note names the parts.
    #[test]
    fn float_text_length_comes_from_the_loaded_rows() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "f".into(),
            data_type: "Float64".into(),
        }];
        t.rows = [3.0, 12.5, 100.0]
            .iter()
            .map(|v| vec![CellValue::Float(*v)])
            .collect();
        let stats = [SummaryStat::TextLenMin, SummaryStat::TextLenMax];
        let want = build_summary_table(&t, &stats).unwrap();
        let mut c = DuckConn::new(t.clone());
        let (got, parts) = run(&mut c, &source(), &t, 3, &stats, &AtomicBool::new(false)).unwrap();
        for col in 2..want.col_count() {
            assert_eq!(
                got.get(0, col).unwrap().to_string(),
                want.get(0, col).unwrap().to_string(),
                "{}",
                want.columns[col].name
            );
        }
        assert_eq!(
            parts.by_design,
            vec!["text_len_min, text_len_max (Float)".to_string()]
        );
        assert!(
            c.log
                .iter()
                .all(|q| !q.contains("LENGTH(") && !q.contains("length(")),
            "{:?}",
            c.log
        );
    }

    #[test]
    fn quartiles_interpolate_on_the_server() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }];
        t.rows = (1..=4).map(|i| vec![CellValue::Int(i)]).collect();
        let stats = [
            SummaryStat::Q25,
            SummaryStat::Median,
            SummaryStat::Q75,
            SummaryStat::Iqr,
        ];
        let mut c = DuckConn::new(t.clone());
        let (s, _) = run(&mut c, &source(), &t, 4, &stats, &AtomicBool::new(false)).unwrap();
        let v = |col: usize| cell_f64(s.get(0, col).unwrap()).unwrap();
        // Columns follow the SummaryStat enum order: median, iqr, q25, q75.
        assert_eq!((v(2), v(3), v(4), v(5)), (2.5, 1.5, 1.75, 3.25));
    }

    /// A refused mode query blanks its cell; a refused quartile query falls back to the loaded rows.
    #[test]
    fn a_refused_mode_or_quartile_query_keeps_the_rest() {
        let t = table();
        let stats = [
            SummaryStat::Min,
            SummaryStat::Mode,
            SummaryStat::Median,
            SummaryStat::NullCount,
        ];
        let mut c = DuckConn::new(t.clone());
        c.fail_on = vec!["octa_v", "PERCENTILE_CONT"];
        let (s, parts) = run(
            &mut c,
            &source(),
            &t,
            t.row_count(),
            &stats,
            &AtomicBool::new(false),
        )
        .unwrap();
        // The refused quartile query is the server's refusal, not the engine's gap.
        assert!(parts.engine.is_empty());
        assert_eq!(parts.failed, vec!["median".to_string()]);
        let col = |name: &str| s.columns.iter().position(|c| c.name == name).unwrap();
        // Row 0 is the numeric column `n`; the aggregate scan still answers.
        let local = build_summary_table(&t, &stats).unwrap();
        for id in ["null_count", "min"] {
            assert_eq!(
                s.get(0, col(id)).unwrap().to_string(),
                local.get(0, col(id)).unwrap().to_string(),
                "{id}"
            );
        }
        // Mode stays blank; the refused median is filled from the loaded rows.
        assert_eq!(s.get(0, col("mode")).unwrap().to_string(), "");
        assert_ne!(local.get(0, col("mode")).unwrap().to_string(), "");
        assert_eq!(
            s.get(0, col("median")).unwrap().to_string(),
            local.get(0, col("median")).unwrap().to_string()
        );
    }

    /// Records every statement and answers with an empty table.
    struct Recorder(DbEngine, Vec<String>);

    impl DbConnector for Recorder {
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
            self.1.push(sql.to_string());
            Ok(DataTable::empty())
        }
        fn engine(&self) -> DbEngine {
            self.0
        }
        fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn write_table(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: DbWriteMode,
            _: &DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            anyhow::bail!("recorder")
        }
    }

    #[test]
    fn sql_server_sql_shape() {
        let mut src = source();
        src.conn.engine = DbEngine::Mssql;
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }];
        let mut c = Recorder(DbEngine::Mssql, vec![]);
        run(
            &mut c,
            &src,
            &t,
            0,
            &[SummaryStat::Sum, SummaryStat::Mode],
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(c.1.iter().any(|s| s.contains("SUM(CAST(")), "{:?}", c.1);
        assert!(
            c.1.iter()
                .any(|s| s.ends_with(" OFFSET 0 ROWS FETCH NEXT 1 ROWS ONLY")),
            "{:?}",
            c.1
        );
    }
}
