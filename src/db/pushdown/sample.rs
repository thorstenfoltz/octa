//! Random sample on the server. **Exact**: every row equally likely; the
//! database reads and shuffles the whole table. **Fast**: the engine's block
//! sampling reads only some blocks, so about `n` rows come back and rows
//! stored together tend to be picked together.

use std::sync::atomic::AtomicBool;

use crate::data::DataTable;
use crate::db::{DbConnector, DbEngine};

use super::dialect::limit_clause;
use super::row_estimate::{estimate_sql, read_estimate};
use super::{ServerSource, check_cancel, count_rows};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleMethod {
    Exact,
    Fast,
}

/// Block sampling reads this many times the share of the table `n` needs,
/// then keeps `n` at random, so an unlucky draw of blocks still usually
/// yields `n` rows.
const FAST_OVERSAMPLE: f64 = 2.0;

/// Past this share of the table, block sampling reads most of it anyway and
/// the exact sample costs no more.
const FAST_MAX_PERCENT: f64 = 50.0;

/// A random number per row, for `ORDER BY`.
fn random_fn(e: DbEngine) -> &'static str {
    match e {
        DbEngine::MySql | DbEngine::Databricks | DbEngine::BigQuery => "RAND()",
        DbEngine::Mssql => "NEWID()",
        DbEngine::Oracle => "DBMS_RANDOM.VALUE",
        DbEngine::ClickHouse => "rand()",
        DbEngine::Postgres
        | DbEngine::Redshift
        | DbEngine::Snowflake
        | DbEngine::Trino
        | DbEngine::Athena
        | DbEngine::Exasol => "RANDOM()",
    }
}

/// `table` (a quoted base table, never a derived one: every engine samples
/// stored blocks) with about `percent` % of its blocks. `None` where the
/// engine has no block sampling: MySQL, Redshift, Exasol, ClickHouse, whose
/// `SAMPLE` works only on tables declared with a sampling key, and
/// Databricks, whose `TABLESAMPLE` picks rows and still reads every one.
fn tablesample(e: DbEngine, table: &str, percent: f64) -> Option<String> {
    let p = format!("{percent:.6}");
    Some(match e {
        DbEngine::Postgres | DbEngine::Snowflake | DbEngine::Trino | DbEngine::Athena => {
            format!("{table} TABLESAMPLE SYSTEM ({p})")
        }
        DbEngine::Mssql => format!("{table} TABLESAMPLE ({p} PERCENT)"),
        DbEngine::BigQuery => format!("{table} TABLESAMPLE SYSTEM ({p} PERCENT)"),
        DbEngine::Oracle => format!("{table} SAMPLE BLOCK ({p})"),
        DbEngine::MySql
        | DbEngine::Redshift
        | DbEngine::ClickHouse
        | DbEngine::Exasol
        | DbEngine::Databricks => {
            return None;
        }
    })
}

/// Whether the engine offers the Fast sample at all.
pub fn fast_available(e: DbEngine) -> bool {
    tablesample(e, "t", 1.0).is_some()
}

/// `n` rows, each equally likely.
pub fn exact_sql(src: &ServerSource, n: usize) -> String {
    let e = src.engine();
    format!(
        "SELECT * FROM {} ORDER BY {}{}",
        src.from_sql(),
        random_fn(e),
        limit_clause(e, n)
    )
}

/// About `n` of `rows` rows by block sampling; `None` when it does not apply:
/// no block sampling on this engine, a filtered or hashed source (a sample of
/// blocks cannot follow a filter or carry hash columns), an empty table, or
/// `n` a large share of it.
pub fn fast_sql(src: &ServerSource, n: usize, rows: usize) -> Option<String> {
    if !src.is_plain() {
        return None;
    }
    let percent = n as f64 * FAST_OVERSAMPLE / rows as f64 * 100.0;
    if percent >= FAST_MAX_PERCENT {
        return None;
    }
    let e = src.engine();
    let t = tablesample(e, &src.table_sql(), percent.max(0.000_001))?;
    Some(format!(
        "SELECT * FROM {t} ORDER BY {}{}",
        random_fn(e),
        limit_clause(e, n)
    ))
}

/// `n` random rows of `src`, at most `cap` (the connectors' row cap); the
/// flag says `n` was cut to `cap`, and the method is the one that ran: Fast
/// falls back to Exact wherever [`fast_sql`] does not apply or its draw found
/// no rows.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    n: usize,
    method: SampleMethod,
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, bool, SampleMethod)> {
    check_cancel(cancel)?;
    let take = n.min(cap).max(1);
    let exact = exact_sql(src, take);
    let fast = match method {
        SampleMethod::Exact => None,
        SampleMethod::Fast => fast_rows(c, src, cancel)?.and_then(|rows| fast_sql(src, take, rows)),
    };
    check_cancel(cancel)?;
    let Some(fast) = fast else {
        return Ok((c.query(&exact)?, n > take, SampleMethod::Exact));
    };
    let t = c.query(&fast)?;
    // A small table can be a handful of blocks, splits or micro-partitions,
    // and drawing none of them returns nothing at all.
    if t.row_count() == 0 {
        check_cancel(cancel)?;
        return Ok((c.query(&exact)?, n > take, SampleMethod::Exact));
    }
    Ok((t, n > take, SampleMethod::Fast))
}

/// The row count block sampling sizes its share by; `None` where Fast does
/// not apply, so Exact runs without counting first: no block sampling on
/// this engine, a filtered or hashed source, or no catalog figure on an
/// engine that keeps one. That last is mostly a view, which Postgres and SQL
/// Server refuse to block-sample. Athena keeps no figure for any table and
/// samples views, so it counts.
fn fast_rows(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    cancel: &AtomicBool,
) -> anyhow::Result<Option<usize>> {
    let e = src.engine();
    if !src.is_plain() || !fast_available(e) {
        return Ok(None);
    }
    match estimate_sql(src) {
        Some(sql) => Ok(c.query(&sql).ok().and_then(|t| read_estimate(e, &t))),
        None => count_rows(c, src, cancel).map(Some),
    }
}

#[cfg(test)]
#[path = "sample_tests.rs"]
mod tests;
