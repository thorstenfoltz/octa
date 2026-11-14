//! Funnel on a database tab: the server finds, step by step, the moment each
//! person reached it and counts the people; only the counts come back and
//! `data::funnel::table` builds the result. Times are compared as epoch
//! seconds, so "within days" is plain arithmetic on every engine.
//! On a server only NULL is empty, and text compares by the server's
//! collation (case-blind on MySQL and SQL Server), as for calculated columns.

use std::sync::atomic::AtomicBool;

use crate::data::funnel::{Counts, check};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{epoch_seconds, str_lit, text};
use super::{ServerSource, cell_i64, check_cancel};

/// What to compute, by column name (quoted inside).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerFunnel<'a> {
    pub who: &'a str,
    pub event: &'a str,
    /// A typed date or timestamp column: a server has no row order.
    pub when: &'a str,
    pub steps: &'a [String],
    pub within_days: Option<u32>,
}

/// The two statements: people per step (`octa_k`, `octa_n`), skipped rows.
pub fn sql(e: DbEngine, from: &str, spec: &ServerFunnel) -> [String; 2] {
    let q = |n: &str| e.quote_ident(n);
    let (w, t) = (q(spec.who), q(spec.when));
    let mut with = format!(
        "WITH x AS (SELECT {w} AS octa_w, {ev} AS octa_e, {secs} AS octa_t FROM {from} \
         WHERE {w} IS NOT NULL AND {t} IS NOT NULL)",
        ev = text(e, &q(spec.event)),
        secs = epoch_seconds(e, &t),
    );
    let mut counts = Vec::new();
    for (k, step) in spec.steps.iter().enumerate() {
        let lit = str_lit(e, step);
        if k == 0 {
            with.push_str(&format!(
                ", s0 AS (SELECT octa_w, MIN(octa_t) AS octa_t, MIN(octa_t) AS octa_t0 \
                 FROM x WHERE octa_e = {lit} GROUP BY octa_w)"
            ));
        } else {
            let p = format!("s{}", k - 1);
            let within = spec.within_days.map_or(String::new(), |d| {
                format!(" AND x.octa_t <= {p}.octa_t0 + {}", u64::from(d) * 86_400)
            });
            with.push_str(&format!(
                ", s{k} AS (SELECT x.octa_w AS octa_w, MIN(x.octa_t) AS octa_t, \
                 MIN({p}.octa_t0) AS octa_t0 FROM x JOIN {p} ON x.octa_w = {p}.octa_w \
                 WHERE x.octa_e = {lit} AND x.octa_t >= {p}.octa_t{within} GROUP BY x.octa_w)"
            ));
        }
        counts.push(format!(
            "SELECT {k} AS octa_k, COUNT(*) AS octa_n FROM s{k}"
        ));
    }
    [
        format!("{with} {}", counts.join(" UNION ALL ")),
        format!("SELECT COUNT(*) FROM {from} WHERE {w} IS NULL OR {t} IS NULL"),
    ]
}

/// Run both statements and gather what `table` needs.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    spec: &ServerFunnel,
    cancel: &AtomicBool,
) -> anyhow::Result<Counts> {
    check(spec.steps, true, spec.within_days)
        .map_err(|e| anyhow::anyhow!("{}", crate::i18n::t(e.key())))?;
    let [main, skipped_sql] = sql(src.engine(), &src.from_sql(), spec);
    check_cancel(cancel)?;
    let t = c.query(&main)?;
    check_cancel(cancel)?;
    let k = c.query(&skipped_sql)?;
    let mut users = vec![0i64; spec.steps.len()];
    for row in &t.rows {
        if let (Some(i), Some(n)) = (
            row.first().and_then(cell_i64),
            row.get(1).and_then(cell_i64),
        ) && let Some(slot) = usize::try_from(i).ok().and_then(|i| users.get_mut(i))
        {
            *slot = n;
        }
    }
    Ok(Counts {
        steps: spec.steps.to_vec(),
        users,
        skipped: k.get(0, 0).and_then(cell_i64).unwrap_or(0).max(0) as usize,
    })
}

#[cfg(test)]
#[path = "funnel_tests.rs"]
mod tests;
