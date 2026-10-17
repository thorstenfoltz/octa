//! A table's row count without reading the table: the catalog's own figure
//! where the engine keeps one (an estimate on most, exact on a few), else
//! `COUNT(*)`. The Join key finder shows it before a whole-table scan.

use std::sync::atomic::AtomicBool;

use super::{ServerSource, cell_f64, check_cancel, count_rows};
use crate::data::{CellValue, DataTable};
use crate::db::relationships::lit;
use crate::db::{DbConnector, DbEngine};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowCount {
    pub rows: usize,
    /// From statistics, not counted: say "about".
    pub estimate: bool,
}

/// The catalog statement for `src`'s row count, or `None` where the engine
/// keeps no figure Octa can use.
pub fn estimate_sql(src: &ServerSource) -> Option<String> {
    // Statistics describe the table, not a filtered result: count instead.
    if src.filter.is_some() {
        return None;
    }
    let engine = src.engine();
    let (s, t) = (lit(&src.schema), lit(&src.table));
    // `catalog.` for the engines that have a catalog level.
    let cat = src
        .catalog
        .as_deref()
        .map(|c| format!("{}.", engine.quote_ident(c)))
        .unwrap_or_default();
    Some(match engine {
        DbEngine::Postgres => format!(
            "SELECT c.reltuples FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n \
             ON n.oid = c.relnamespace WHERE n.nspname = {s} AND c.relname = {t}"
        ),
        DbEngine::Redshift => format!(
            "SELECT tbl_rows FROM svv_table_info WHERE \"schema\" = {s} AND \"table\" = {t}"
        ),
        DbEngine::MySql => format!(
            "SELECT TABLE_ROWS FROM information_schema.TABLES WHERE TABLE_SCHEMA = {s} AND TABLE_NAME = {t}"
        ),
        DbEngine::Mssql => format!(
            "SELECT SUM(p.rows) FROM sys.partitions p JOIN sys.tables tb ON tb.object_id = p.object_id \
             JOIN sys.schemas sc ON sc.schema_id = tb.schema_id \
             WHERE sc.name = {s} AND tb.name = {t} AND p.index_id IN (0, 1)"
        ),
        DbEngine::Oracle => {
            format!("SELECT NUM_ROWS FROM ALL_TABLES WHERE OWNER = {s} AND TABLE_NAME = {t}")
        }
        DbEngine::ClickHouse => {
            format!("SELECT total_rows FROM system.tables WHERE database = {s} AND name = {t}")
        }
        DbEngine::Exasol => format!(
            "SELECT TABLE_ROW_COUNT FROM EXA_ALL_TABLES WHERE TABLE_SCHEMA = {s} AND TABLE_NAME = {t}"
        ),
        DbEngine::Snowflake => format!(
            "SELECT ROW_COUNT FROM {cat}INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = {s} AND TABLE_NAME = {t}"
        ),
        DbEngine::BigQuery => format!(
            "SELECT row_count FROM {cat}{}.__TABLES__ WHERE table_id = {t}",
            engine.quote_ident(&src.schema)
        ),
        DbEngine::Trino => format!("SHOW STATS FOR {}", src.from_sql()),
        DbEngine::Athena | DbEngine::Databricks => return None,
    })
}

/// The catalog figure is exact rather than an estimate.
pub fn exact_metadata(engine: DbEngine) -> bool {
    matches!(
        engine,
        DbEngine::ClickHouse | DbEngine::Exasol | DbEngine::Snowflake
    )
}

/// The figure in a metadata answer. Zero, negative (Postgres' "never
/// analysed" -1) and NULL read as "no figure", so a fresh table is counted
/// instead of shown as empty. Trino's figure is in the summary row (first
/// cell NULL), column 4.
pub fn read_estimate(engine: DbEngine, t: &DataTable) -> Option<usize> {
    let cell = match engine {
        DbEngine::Trino => (0..t.row_count())
            .find(|&r| matches!(t.get(r, 0), Some(CellValue::Null)))
            .and_then(|r| t.get(r, 4)),
        _ => t.get(0, 0),
    }?;
    cell_f64(cell)
        .filter(|n| *n >= 1.0)
        .map(|n| n.round() as usize)
}

/// `src`'s row count: the catalog figure when there is one (flagged as an
/// estimate unless the engine keeps it exact), else `COUNT(*)`.
pub fn row_count(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    cancel: &AtomicBool,
) -> anyhow::Result<RowCount> {
    check_cancel(cancel)?;
    let engine = src.engine();
    // A catalog query the user may not read (svv_table_info, sys.partitions)
    // is not an error: COUNT(*) still answers.
    let meta = estimate_sql(src)
        .and_then(|sql| c.query(&sql).ok())
        .and_then(|t| read_estimate(engine, &t));
    Ok(match meta {
        Some(rows) => RowCount {
            rows,
            estimate: !exact_metadata(engine),
        },
        None => RowCount {
            rows: count_rows(c, src, cancel)?,
            estimate: false,
        },
    })
}

#[cfg(test)]
#[path = "row_estimate_tests.rs"]
mod tests;
