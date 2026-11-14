//! Cohorts on a database tab: the server finds each person's first period
//! and counts people (or sums a column) per (cohort, period); only that
//! small result comes back and `data::cohort::wide` builds the table.
//! On a server only NULL is empty, as for calculated columns.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::AtomicBool;

use crate::data::cohort::{Cells, Period, Show, date_of};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{date_bucket, sum_operand};
use super::{ServerSource, cell_f64, cell_i64, check_cancel};

/// What to compute, by column name (quoted inside).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerCohort<'a> {
    pub who: &'a str,
    pub when: &'a str,
    /// The column a Sum adds up.
    pub sum: Option<&'a str>,
    /// The tab's type for `sum`: a decimal adds exactly.
    pub sum_type: &'a str,
    pub period: Period,
    pub show: Show,
}

/// The three statements: cells, cohort sizes, skipped rows.
pub fn sql(e: DbEngine, from: &str, spec: &ServerCohort) -> [String; 3] {
    let q = |n: &str| e.quote_ident(n);
    let (w, t) = (q(spec.who), q(spec.when));
    let b = date_bucket(e, &t, spec.period.interval());
    let v = match spec.sum {
        Some(s) => format!(", {} AS octa_v", sum_operand(e, &q(s), spec.sum_type)),
        None => String::new(),
    };
    let with = format!(
        "WITH x AS (SELECT {w} AS octa_w, {b} AS octa_b{v} FROM {from} \
         WHERE {w} IS NOT NULL AND {t} IS NOT NULL), \
         f AS (SELECT octa_w, MIN(octa_b) AS octa_c FROM x GROUP BY octa_w)"
    );
    let agg = if spec.show == Show::Sum {
        "SUM(x.octa_v)"
    } else {
        "COUNT(DISTINCT x.octa_w)"
    };
    [
        format!(
            "{with} SELECT f.octa_c, x.octa_b, {agg} AS octa_n FROM x \
             JOIN f ON x.octa_w = f.octa_w GROUP BY f.octa_c, x.octa_b"
        ),
        format!("{with} SELECT octa_c, COUNT(*) AS octa_n FROM f GROUP BY octa_c"),
        format!("SELECT COUNT(*) FROM {from} WHERE {w} IS NULL OR {t} IS NULL"),
    ]
}

/// Run the three statements and gather what `wide` needs.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    spec: &ServerCohort,
    cancel: &AtomicBool,
) -> anyhow::Result<Cells> {
    if spec.show == Show::Sum && spec.sum.is_none() {
        anyhow::bail!("{}", crate::i18n::t("cohort.err_no_sum"));
    }
    let [main, sizes_sql, skipped_sql] = sql(src.engine(), &src.from_sql(), spec);
    check_cancel(cancel)?;
    let t = c.query(&main)?;
    check_cancel(cancel)?;
    let s = c.query(&sizes_sql)?;
    check_cancel(cancel)?;
    let k = c.query(&skipped_sql)?;
    // `period.start` re-normalises what each engine returns: a date, or a
    // timestamp at midnight.
    let p = spec.period;
    let mut values = HashMap::new();
    let mut last = None;
    for row in &t.rows {
        let (Some(co), Some(b)) = (row.first().and_then(date_of), row.get(1).and_then(date_of))
        else {
            continue;
        };
        let b = p.start(b);
        values.insert(
            (p.start(co), b),
            row.get(2).and_then(cell_f64).unwrap_or(0.0),
        );
        last = last.max(Some(b));
    }
    let mut sizes = BTreeMap::new();
    for row in &s.rows {
        if let (Some(co), Some(n)) = (row.first().and_then(date_of), row.get(1).and_then(cell_i64))
        {
            sizes.insert(p.start(co), n);
        }
    }
    let last = last.ok_or_else(|| anyhow::anyhow!("{}", crate::i18n::t("cohort.err_no_rows")))?;
    Ok(Cells {
        period: p,
        show: spec.show,
        sizes,
        values,
        last,
        skipped: k.get(0, 0).and_then(cell_i64).unwrap_or(0).max(0) as usize,
    })
}

#[cfg(test)]
#[path = "cohort_tests.rs"]
mod tests;
