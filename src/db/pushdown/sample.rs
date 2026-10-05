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
mod tests {
    use super::super::join_keys::tests::text_table;
    use super::super::test_support::{DuckConn, source};
    use super::*;

    fn ids(n: usize) -> DataTable {
        let rows: Vec<Vec<String>> = (0..n).map(|i| vec![i.to_string()]).collect();
        let refs: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(String::as_str).collect())
            .collect();
        let refs: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
        text_table(&["id"], &refs)
    }

    #[test]
    fn the_exact_sample_has_n_distinct_rows_of_the_table() {
        let mut c = DuckConn::new(ids(200));
        let stop = AtomicBool::new(false);
        let (t, capped, _) = run(&mut c, &source(), 30, SampleMethod::Exact, 1_000, &stop).unwrap();
        assert!(!capped);
        let mut got: Vec<String> = t.rows.iter().map(|r| r[0].to_string()).collect();
        assert_eq!(got.len(), 30);
        got.sort();
        got.dedup();
        assert_eq!(got.len(), 30, "no row twice");
        assert!(
            got.iter()
                .all(|v| v.parse::<usize>().is_ok_and(|i| i < 200))
        );
        let first: Vec<String> = (0..30).map(|i| i.to_string()).collect();
        let mut first_sorted = first.clone();
        first_sorted.sort();
        assert_ne!(got, first_sorted, "shuffled, not the first 30 rows");
    }

    #[test]
    fn asking_for_more_than_the_cap_says_so() {
        let mut c = DuckConn::new(ids(50));
        let (t, capped, _) = run(
            &mut c,
            &source(),
            40,
            SampleMethod::Exact,
            10,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(capped);
        assert_eq!(t.row_count(), 10);
    }

    /// DuckDB has no Postgres catalog figure, as a view has none: Fast runs
    /// Exact straight away, without counting the whole view first.
    #[test]
    fn no_catalog_figure_runs_exact_without_counting() {
        let mut c = DuckConn::new(ids(20));
        let (t, _, ran) = run(
            &mut c,
            &source(),
            15,
            SampleMethod::Fast,
            1_000,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(t.row_count(), 15);
        assert_eq!(ran, SampleMethod::Exact);
        assert!(!c.log.iter().any(|q| q.contains("COUNT(*)")), "{:?}", c.log);
    }

    #[test]
    fn a_large_share_or_a_filter_has_no_block_sample() {
        let mut filtered = source();
        filtered.filter = Some("1 = 1".into());
        assert!(fast_sql(&filtered, 10, 1_000_000).is_none());
        assert!(fast_sql(&source(), 10, 1_000_000).is_some());
        assert!(fast_sql(&source(), 15, 20).is_none(), "150 % of the table");
        assert!(fast_sql(&source(), 10, 0).is_none());
    }

    /// Answers a catalog figure or `COUNT(*)` of a million rows, block
    /// sampling with one row only when `.2` is set, anything else with one
    /// row; records every statement.
    struct Script(DbEngine, Vec<String>, bool);

    impl DbConnector for Script {
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
            self.1.push(sql.to_string());
            let mut t = DataTable::empty();
            t.columns = vec![crate::data::ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            }];
            if self.2 || !sql.contains("TABLESAMPLE") {
                t.rows = vec![vec![crate::data::CellValue::Int(1_000_000)]];
            }
            Ok(t)
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
            _: crate::db::DbWriteMode,
            _: &DataTable,
        ) -> anyhow::Result<crate::db::DbWriteReport> {
            anyhow::bail!("read-only test connector")
        }
    }

    #[test]
    fn a_block_sample_that_draws_nothing_runs_exact() {
        let mut c = Script(DbEngine::Postgres, vec![], false);
        let stop = AtomicBool::new(false);
        let (t, _, ran) = run(&mut c, &source(), 10, SampleMethod::Fast, 1_000, &stop).unwrap();
        assert_eq!(t.row_count(), 1);
        assert_eq!(ran, SampleMethod::Exact);
        assert_eq!(c.1.len(), 3, "catalog, block sample, exact: {:?}", c.1);
        assert!(c.1[1].contains("TABLESAMPLE SYSTEM"));
        assert_eq!(c.1[2], exact_sql(&source(), 10));
        assert!(!c.1.iter().any(|q| q.contains("COUNT(*)")));
    }

    /// Athena keeps no catalog figure for any table, so Fast counts.
    #[test]
    fn athena_counts_before_block_sampling() {
        let mut c = Script(DbEngine::Athena, vec![], true);
        let mut src = source();
        src.conn.engine = DbEngine::Athena;
        let (_, _, ran) = run(
            &mut c,
            &src,
            10,
            SampleMethod::Fast,
            1_000,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(ran, SampleMethod::Fast);
        assert_eq!(c.1.len(), 2, "{:?}", c.1);
        assert!(c.1[0].contains("COUNT(*)"), "{:?}", c.1);
        assert!(c.1[1].contains("TABLESAMPLE SYSTEM"));
    }

    #[test]
    fn every_engine_spells_block_sampling_or_has_none() {
        let t = |e| tablesample(e, "t", 0.5);
        assert_eq!(
            t(DbEngine::Postgres).unwrap(),
            "t TABLESAMPLE SYSTEM (0.500000)"
        );
        assert_eq!(
            t(DbEngine::Mssql).unwrap(),
            "t TABLESAMPLE (0.500000 PERCENT)"
        );
        assert_eq!(t(DbEngine::Oracle).unwrap(), "t SAMPLE BLOCK (0.500000)");
        assert_eq!(
            t(DbEngine::BigQuery).unwrap(),
            "t TABLESAMPLE SYSTEM (0.500000 PERCENT)"
        );
        for e in [DbEngine::Snowflake, DbEngine::Trino, DbEngine::Athena] {
            assert_eq!(t(e).unwrap(), "t TABLESAMPLE SYSTEM (0.500000)", "{e:?}");
        }
        for e in [
            DbEngine::MySql,
            DbEngine::Redshift,
            DbEngine::ClickHouse,
            DbEngine::Exasol,
            DbEngine::Databricks,
        ] {
            assert!(t(e).is_none(), "{e:?}");
            assert!(!fast_available(e));
        }
        let sql = fast_sql(&source(), 10, 1_000).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"data\" TABLESAMPLE SYSTEM (2.000000) ORDER BY RANDOM() LIMIT 10"
        );
    }

    #[test]
    fn every_engine_shuffles_with_its_own_random_function() {
        use DbEngine::*;
        for (e, f) in [
            (Postgres, "RANDOM()"),
            (MySql, "RAND()"),
            (Mssql, "NEWID()"),
            (Oracle, "DBMS_RANDOM.VALUE"),
            (Redshift, "RANDOM()"),
            (ClickHouse, "rand()"),
            (Exasol, "RANDOM()"),
            (Trino, "RANDOM()"),
            (Athena, "RANDOM()"),
            (Snowflake, "RANDOM()"),
            (Databricks, "RAND()"),
            (BigQuery, "RAND()"),
        ] {
            let mut src = source();
            src.conn.engine = e;
            assert!(
                exact_sql(&src, 5).contains(&format!(" ORDER BY {f}")),
                "{e:?}"
            );
        }
    }
}
