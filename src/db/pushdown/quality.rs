//! The data-quality report on a database tab, hybrid by design: what is a
//! count (null percentage, distinct ratio, outliers, and the score built
//! from them) is counted on the server over every row; what looks at
//! individual values (type consistency, PII, Benford's, calendar gaps,
//! value shapes, and the extra section tabs) has no sensible SQL form, so
//! it comes from the loaded rows, and the result note names those parts.

use std::sync::atomic::AtomicBool;

use crate::data::DataTable;
use crate::data::quality::{
    QualityReport, ServerCounts, apply_server_counts, build_quality_report, is_numeric_type,
};
use crate::db::DbConnector;

use super::dialect::{as_float, distinct_key, float_lit, quartiles_sql};
use super::{LocalParts, ServerSource, cell_f64, cell_i64, check_cancel, per_column_aggregates};

/// Columns of the report that always come from the loaded rows.
const PATTERN_COLUMNS: &[&str] = &[
    "type_consistency",
    "pii_flag",
    "pii_kind",
    "benford_verdict",
    "calendar_verdict",
    "shape_verdict",
];

/// Outliers by the IQR fence, k = 1.5, as the in-memory method does.
fn server_outliers(
    c: &mut dyn DbConnector,
    e: crate::db::DbEngine,
    from: &str,
    q: &str,
    quartiles: &str,
    cancel: &AtomicBool,
) -> anyhow::Result<i64> {
    let t = c.query(quartiles)?;
    let (Some(q1), Some(q3)) = (
        t.get(0, 0).and_then(cell_f64),
        t.get(0, 2).and_then(cell_f64),
    ) else {
        return Ok(0);
    };
    let iqr = q3 - q1;
    let (lo, hi) = (q1 - 1.5 * iqr, q3 + 1.5 * iqr);
    if !(lo.is_finite() && hi.is_finite()) {
        return Ok(0);
    }
    let f = as_float(e, q);
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT SUM(CASE WHEN {f} < {} OR {f} > {} THEN 1 ELSE 0 END) FROM {from}",
        float_lit(lo),
        float_lit(hi)
    ))?;
    Ok(t.get(0, 0).and_then(cell_i64).unwrap_or(0))
}

pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    loaded: &DataTable,
    total: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(QualityReport, LocalParts)> {
    let e = src.engine();
    let from = src.from_sql();
    let mut report = build_quality_report(loaded)?;
    // The section tabs are local too; each carries its own partial note.
    // Named by the title the user sees on each tab.
    let mut local = LocalParts {
        by_design: PATTERN_COLUMNS
            .iter()
            .map(|s| s.to_string())
            .chain(report.sections.iter().map(|s| crate::i18n::t(&s.title_key)))
            .collect(),
        ..Default::default()
    };

    let per_col: Vec<Vec<String>> = loaded
        .columns
        .iter()
        .map(|col| {
            let q = e.quote_ident(&col.name);
            vec![
                format!("COUNT({q})"),
                format!("COUNT(DISTINCT {})", distinct_key(e, &q, &col.data_type)),
            ]
        })
        .collect();
    let base = per_column_aggregates(c, &from, &per_col, cancel)?;

    let (mut no_quartiles, mut refused) = (false, false);
    let mut counts = Vec::with_capacity(base.len());
    for (col, cells) in loaded.columns.iter().zip(base) {
        let Some(cells) = cells else {
            counts.push(None);
            continue;
        };
        let non_null = cells.first().and_then(cell_i64).unwrap_or(0);
        let distinct = cells.get(1).and_then(cell_i64).unwrap_or(0);
        let mut outliers = Some(0);
        if is_numeric_type(&col.data_type) && non_null >= 4 {
            let q = e.quote_ident(&col.name);
            check_cancel(cancel)?;
            outliers = match quartiles_sql(e, &from, &q) {
                None => {
                    no_quartiles = true;
                    None
                }
                Some(sql) => match server_outliers(c, e, &from, &q, &sql, cancel) {
                    Ok(n) => Some(n),
                    Err(e) => {
                        // A user Cancel must not read as "this column failed".
                        check_cancel(cancel)?;
                        tracing::debug!("pushdown quality: outliers of {} failed: {e:#}", col.name);
                        refused = true;
                        None
                    }
                },
            };
        }
        counts.push(Some(ServerCounts {
            non_null,
            distinct,
            outliers,
        }));
    }
    if no_quartiles {
        local.engine.push("outlier_count".to_string());
    }
    if refused {
        local.failed.push("outlier_count".to_string());
    }
    apply_server_counts(&mut report, &counts, total, loaded.row_count());
    Ok((report, local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo};
    use crate::db::pushdown::test_support::{DuckConn, source};

    /// The server holds 100 rows, the tab 10. The counted columns must
    /// describe the 100; the pattern columns are named as local.
    #[test]
    fn counts_come_from_the_server_and_patterns_are_named_local() {
        let mut full = DataTable::empty();
        full.columns = vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }];
        full.rows = (0..100)
            .map(|i| {
                vec![if i < 25 {
                    CellValue::Null
                } else {
                    CellValue::Int(i % 10)
                }]
            })
            .collect();
        full.rows[99] = vec![CellValue::Int(10_000)];
        let mut loaded = full.clone();
        loaded.rows.truncate(10);

        let mut c = DuckConn::new(full);
        let (report, local) =
            run(&mut c, &source(), &loaded, 100, &AtomicBool::new(false)).unwrap();
        let ids = crate::data::quality::quality_column_ids();
        let col = |id: &str| ids.iter().position(|x| *x == id).unwrap();
        let cell = |id: &str| report.table.get(0, col(id)).unwrap().to_string();
        assert_eq!(cell("null_percentage"), "25");
        // 11 distinct values (0..=9 and 10000) over 75 present cells.
        assert!(cell("distinct_ratio").starts_with("0.1466"));
        assert_eq!(cell("outlier_count"), "1");
        for id in [
            "type_consistency",
            "pii_flag",
            "pii_kind",
            "benford_verdict",
            "calendar_verdict",
            "shape_verdict",
        ] {
            assert!(
                local.by_design.iter().any(|l| l == id),
                "{id} missing from {local:?}"
            );
        }
        // Those plus one entry per section tab, by the title on the tab.
        assert_eq!(
            local.by_design.len(),
            PATTERN_COLUMNS.len() + report.sections.len()
        );
        for sec in &report.sections {
            let title = crate::i18n::t(&sec.title_key);
            assert!(local.by_design.contains(&title), "{title}");
        }
        assert!(local.engine.is_empty());
        // Score from the server counts over 100 rows, with the loaded rows'
        // type consistency.
        let tc =
            crate::db::pushdown::cell_f64(report.table.get(0, col("type_consistency")).unwrap())
                .unwrap();
        let want = crate::data::quality::column_score(0.25, 11.0 / 75.0, tc, 1, 100);
        assert_eq!(
            crate::db::pushdown::cell_f64(report.table.get(0, col("score")).unwrap()),
            Some(want)
        );
    }

    /// MySQL has no percentile: the loaded outlier count stays, named local,
    /// and is penalised against the loaded rows, not the server's total.
    #[test]
    fn without_quartiles_the_loaded_outliers_are_scored_over_loaded_rows() {
        let mut full = DataTable::empty();
        full.columns = vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }];
        full.rows = (0..100).map(|i| vec![CellValue::Int(i % 10)]).collect();
        let mut loaded = DataTable::empty();
        loaded.columns = full.columns.clone();
        loaded.rows = (0..10)
            .map(|i| vec![CellValue::Int(if i == 9 { 10_000 } else { i % 3 })])
            .collect();

        let mut src = source();
        src.conn.engine = crate::db::DbEngine::MySql;
        let mut c = DuckConn::new(full);
        let (report, local) = run(&mut c, &src, &loaded, 100, &AtomicBool::new(false)).unwrap();
        let ids = crate::data::quality::quality_column_ids();
        let cell = |id: &str| {
            let i = ids.iter().position(|x| *x == id).unwrap();
            report.table.get(0, i).unwrap().to_string()
        };
        assert_eq!(local.engine, vec!["outlier_count".to_string()]);
        let outliers: usize = cell("outlier_count").parse().unwrap();
        assert!(outliers >= 1);
        let tc: f64 = cell("type_consistency").parse().unwrap();
        let want = crate::data::quality::column_score(0.0, 0.1, tc, outliers, 10);
        assert_eq!(cell("score"), format!("{want}"));
    }

    /// A refused outlier query blanks only the outlier count; the null
    /// percentage and distinct ratio still come from the server.
    #[test]
    fn a_refused_outlier_query_keeps_the_rest() {
        let mut full = DataTable::empty();
        full.columns = vec![ColumnInfo {
            name: "x".into(),
            data_type: "Int64".into(),
        }];
        full.rows = (0..20)
            .map(|i| {
                vec![if i < 5 {
                    CellValue::Null
                } else {
                    CellValue::Int(i)
                }]
            })
            .collect();
        let mut c = DuckConn::new(full.clone());
        c.fail_on = vec!["PERCENTILE_CONT"];
        let (report, local) = run(&mut c, &source(), &full, 20, &AtomicBool::new(false)).unwrap();
        let ids = crate::data::quality::quality_column_ids();
        let cell = |id: &str| {
            let i = ids.iter().position(|x| *x == id).unwrap();
            report.table.get(0, i).unwrap().to_string()
        };
        assert_eq!(cell("null_percentage"), "25");
        assert_eq!(cell("distinct_ratio"), "1");
        // The server refused: not the engine's gap, so its own list.
        assert!(local.engine.is_empty());
        assert_eq!(local.failed, vec!["outlier_count".to_string()]);
    }
}
