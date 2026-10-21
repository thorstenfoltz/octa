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
    shapes: crate::data::shapes::ShapeDetail,
    cancel: &AtomicBool,
) -> anyhow::Result<(QualityReport, LocalParts)> {
    let e = src.engine();
    let from = src.from_sql();
    let mut report = build_quality_report(loaded, shapes)?;
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
#[path = "quality_tests.rs"]
mod tests;
