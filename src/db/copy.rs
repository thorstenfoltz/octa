//! Server-to-server table copy, any engine to any engine, via two lanes
//! ([`choose_lane`]):
//!
//! - **Fast** (both engines DuckDB-attachable, i.e. Postgres/Redshift/MySQL):
//!   ATTACH source read-only + target writable in one in-memory DuckDB and
//!   `INSERT INTO tgt SELECT * FROM src`. The data streams server-to-server
//!   without ever materialising in Octa's table model, the postgres extension
//!   writes via binary COPY, and it is unaffected by the initial-load row cap.
//!   The DuckDB extensions install over the network on first use (then cached).
//! - **Universal** (any other pair, incl. the warehouses and SQL Server): pull
//!   batches from the source through [`DbConnector::fetch_batches`] and write
//!   each to the target via [`DbConnector::write_table`]. Slower (data passes
//!   through Octa) but works for every engine.

use anyhow::{Context, bail};

use crate::sql::duckdb_attach_sql;

use super::{DbConnection, DbConnector, DbEngine, DbWriteMode, auth, ensure_write_allowed};

/// Which copy strategy a source->target pair uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyLane {
    /// Both engines are DuckDB-attachable: stream server-to-server through an
    /// in-memory DuckDB (binary COPY, uncapped, fastest).
    Fast,
    /// At least one engine is not attachable: pull batches from the source and
    /// write them to the target through Octa (works for any pair).
    Universal,
}

/// Rows per batch on the universal lane.
const COPY_BATCH_ROWS: usize = 50_000;

/// Pick the copy lane for an engine pair: [`CopyLane::Fast`] only when both
/// sides are DuckDB-attachable, else [`CopyLane::Universal`].
pub fn choose_lane(src: DbEngine, tgt: DbEngine) -> CopyLane {
    if src.duckdb_attachable() && tgt.duckdb_attachable() {
        CopyLane::Fast
    } else {
        CopyLane::Universal
    }
}

/// Outcome of a [`copy_table`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCopyReport {
    pub rows_copied: usize,
    /// Whether the target table was created by this copy.
    pub created: bool,
}

/// One side of the copy.
#[derive(Debug, Clone)]
pub struct DbCopyEnd {
    pub conn: DbConnection,
    /// Catalog for a three-level engine (Snowflake/Databricks/BigQuery), else
    /// None. Only meaningful for the source on the universal lane.
    pub catalog: Option<String>,
    pub schema: String,
    pub table: String,
}

/// `"alias"."schema"."table"` with DuckDB double-quote quoting.
fn qualified(alias: &str, schema: &str, table: &str) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    format!("{}.{}.{}", q(alias), q(schema), q(table))
}

/// The statements run after both ATTACHes, in order. Pure for unit tests.
/// Create/Replace build the target's schema from the source via a LIMIT 0
/// CTAS, then a separate INSERT..SELECT carries the rows (its execute()
/// return is the row count, which a plain CTAS would not report).
pub(crate) fn copy_statements(mode: DbWriteMode, src: &str, tgt: &str) -> Vec<String> {
    let mut out = Vec::new();
    if matches!(mode, DbWriteMode::Replace) {
        out.push(format!("DROP TABLE IF EXISTS {tgt}"));
    }
    if matches!(mode, DbWriteMode::Create | DbWriteMode::Replace) {
        out.push(format!("CREATE TABLE {tgt} AS SELECT * FROM {src} LIMIT 0"));
    }
    out.push(format!("INSERT INTO {tgt} SELECT * FROM {src}"));
    out
}

/// Copy `source.schema.table` into `target.schema.table` in one streaming
/// pass. `mode` follows [`DbWriteMode`]: Create (table must not exist),
/// Append (must exist, columns compatible), Replace (drop + recreate). The
/// target connection's `allow_writes` gate is enforced; secrets are the
/// resolved keyring/token passwords (token auth may shell out to a cloud
/// CLI, so call this off the UI thread).
pub fn copy_table(
    source: &DbCopyEnd,
    source_secret: Option<&str>,
    target: &DbCopyEnd,
    target_secret: Option<&str>,
    mode: DbWriteMode,
) -> anyhow::Result<DbCopyReport> {
    ensure_write_allowed(&target.conn, None)?;
    if source.conn.id == target.conn.id
        && source.schema == target.schema
        && source.table == target.table
    {
        bail!("source and target are the same table");
    }
    match choose_lane(source.conn.engine, target.conn.engine) {
        CopyLane::Fast => copy_fast(source, source_secret, target, target_secret, mode),
        CopyLane::Universal => copy_universal(source, source_secret, target, target_secret, mode),
    }
}

/// The fast lane: both engines attachable, stream through one in-memory DuckDB.
/// Guards are checked by [`copy_table`]; the lane choice guarantees both sides
/// are attachable.
fn copy_fast(
    source: &DbCopyEnd,
    source_secret: Option<&str>,
    target: &DbCopyEnd,
    target_secret: Option<&str>,
    mode: DbWriteMode,
) -> anyhow::Result<DbCopyReport> {
    let src_pass = auth::resolve_password(&source.conn, source_secret)?;
    let tgt_pass = auth::resolve_password(&target.conn, target_secret)?;

    let duck = duckdb::Connection::open_in_memory().context("opening in-memory DuckDB")?;
    for engine in [source.conn.engine, target.conn.engine] {
        let ext = match engine {
            DbEngine::Postgres | DbEngine::Redshift => "postgres",
            DbEngine::MySql => "mysql",
            _ => unreachable!("choose_lane routes non-attachable engines to the universal lane"),
        };
        duck.execute_batch(&format!("INSTALL {ext}; LOAD {ext};"))
            .with_context(|| {
                format!("installing the DuckDB {ext} extension (network on first use)")
            })?;
    }
    duck.execute(&duckdb_attach_sql(&source.conn, &src_pass, "src", true), [])
        .with_context(|| format!("attaching source '{}'", source.conn.name))?;
    duck.execute(
        &duckdb_attach_sql(&target.conn, &tgt_pass, "tgt", false),
        [],
    )
    .with_context(|| format!("attaching target '{}'", target.conn.name))?;

    let src_ref = qualified("src", &source.schema, &source.table);
    let tgt_ref = qualified("tgt", &target.schema, &target.table);
    let mut rows = 0usize;
    for stmt in copy_statements(mode, &src_ref, &tgt_ref) {
        rows = duck
            .execute(&stmt, [])
            .with_context(|| format!("running: {stmt}"))?;
    }
    Ok(DbCopyReport {
        rows_copied: rows,
        created: matches!(mode, DbWriteMode::Create | DbWriteMode::Replace),
    })
}

/// The universal lane: connect to both engines and stream batches from source
/// to target through Octa. Works for any pair (the only lane that reaches the
/// warehouse engines). Guards are checked by [`copy_table`].
///
/// ponytail: an *empty* source yields no batches, so a Create/Replace target
/// is not created (write_table needs a batch to learn the columns). Copying an
/// empty table via this lane is a no-op; upgrade by materialising an empty
/// schema first if it ever matters.
pub(crate) fn copy_universal(
    source: &DbCopyEnd,
    source_secret: Option<&str>,
    target: &DbCopyEnd,
    target_secret: Option<&str>,
    mode: DbWriteMode,
) -> anyhow::Result<DbCopyReport> {
    let mut src = super::connect(&source.conn, source_secret)
        .with_context(|| format!("connecting source '{}'", source.conn.name))?;
    let mut tgt = super::connect(&target.conn, target_secret)
        .with_context(|| format!("connecting target '{}'", target.conn.name))?;
    let e = source.conn.engine;
    let q = |s: &str| e.quote_ident(s);
    let src_name = match &source.catalog {
        Some(c) => format!("{}.{}.{}", q(c), q(&source.schema), q(&source.table)),
        None => format!("{}.{}", q(&source.schema), q(&source.table)),
    };
    let select = format!("SELECT * FROM {src_name}");
    let rows = copy_batches(
        src.as_mut(),
        tgt.as_mut(),
        &select,
        &target.schema,
        &target.table,
        mode,
    )?;
    Ok(DbCopyReport {
        rows_copied: rows,
        created: matches!(mode, DbWriteMode::Create | DbWriteMode::Replace),
    })
}

/// Pull `select_sql` from `source` in batches and write each to `target`. The
/// first batch uses `mode` (Create/Replace/Append); every later batch forces
/// Append so a multi-batch copy builds one table. Returns rows written.
pub(crate) fn copy_batches(
    source: &mut dyn DbConnector,
    target: &mut dyn DbConnector,
    select_sql: &str,
    tgt_schema: &str,
    tgt_table: &str,
    mode: DbWriteMode,
) -> anyhow::Result<usize> {
    let mut written = 0usize;
    let mut first = true;
    source.fetch_batches(select_sql, COPY_BATCH_ROWS, &mut |batch| {
        let m = if std::mem::replace(&mut first, false) {
            mode
        } else {
            DbWriteMode::Append
        };
        let report = target.write_table(None, tgt_schema, tgt_table, m, &batch)?;
        written += report.rows_written;
        Ok(())
    })?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbWriteReport;

    fn conn(engine: DbEngine, allow_writes: bool) -> DbConnection {
        DbConnection {
            id: format!("db-{engine:?}"),
            name: format!("{engine:?}"),
            engine,
            host: "h".into(),
            port: engine.default_port(),
            database: "d".into(),
            username: "u".into(),
            auth: super::super::DbAuth::Password,
            allow_writes,
            oauth_client_id: None,
            oauth_tenant: None,
        }
    }

    fn end(engine: DbEngine, allow_writes: bool) -> DbCopyEnd {
        DbCopyEnd {
            conn: conn(engine, allow_writes),
            catalog: None,
            schema: "public".into(),
            table: "t".into(),
        }
    }

    #[test]
    fn universal_source_name_qualifies_catalog() {
        // Three-part backtick form for a catalog engine source.
        let e = DbEngine::BigQuery;
        let q = |s: &str| e.quote_ident(s);
        assert_eq!(
            format!("{}.{}.{}", q("proj"), q("ds"), q("t")),
            "`proj`.`ds`.`t`"
        );
        // Two-part when no catalog.
        let e = DbEngine::Postgres;
        let q = |s: &str| e.quote_ident(s);
        assert_eq!(format!("{}.{}", q("public"), q("t")), "\"public\".\"t\"");
    }

    #[test]
    fn statements_per_mode() {
        assert_eq!(
            copy_statements(DbWriteMode::Create, "s", "t"),
            vec![
                "CREATE TABLE t AS SELECT * FROM s LIMIT 0".to_string(),
                "INSERT INTO t SELECT * FROM s".to_string(),
            ]
        );
        assert_eq!(
            copy_statements(DbWriteMode::Append, "s", "t"),
            vec!["INSERT INTO t SELECT * FROM s".to_string()]
        );
        assert_eq!(
            copy_statements(DbWriteMode::Replace, "s", "t"),
            vec![
                "DROP TABLE IF EXISTS t".to_string(),
                "CREATE TABLE t AS SELECT * FROM s LIMIT 0".to_string(),
                "INSERT INTO t SELECT * FROM s".to_string(),
            ]
        );
    }

    #[test]
    fn qualified_quotes_every_part() {
        assert_eq!(
            qualified("src", "we\"ird", "my table"),
            "\"src\".\"we\"\"ird\".\"my table\""
        );
    }

    #[test]
    fn lane_selection() {
        assert_eq!(
            choose_lane(DbEngine::Postgres, DbEngine::MySql),
            CopyLane::Fast
        );
        assert_eq!(
            choose_lane(DbEngine::Postgres, DbEngine::Redshift),
            CopyLane::Fast
        );
        assert_eq!(
            choose_lane(DbEngine::Postgres, DbEngine::Snowflake),
            CopyLane::Universal
        );
        assert_eq!(
            choose_lane(DbEngine::Snowflake, DbEngine::Databricks),
            CopyLane::Universal
        );
        // MSSQL is no longer rejected: it rides the universal lane.
        assert_eq!(
            choose_lane(DbEngine::Mssql, DbEngine::Postgres),
            CopyLane::Universal
        );
    }

    /// A fake source yielding `batches` one-row batches (overrides the default
    /// paging so no real query runs).
    struct FakeSource {
        batches: usize,
    }
    impl DbConnector for FakeSource {
        fn engine(&self) -> DbEngine {
            DbEngine::Postgres
        }
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, _: &str) -> anyhow::Result<crate::data::DataTable> {
            unreachable!("fetch_batches is overridden")
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
            _: &crate::data::DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            unreachable!("source is never written")
        }
        fn fetch_batches(
            &mut self,
            _sql: &str,
            _n: usize,
            sink: &mut dyn FnMut(crate::data::DataTable) -> anyhow::Result<()>,
        ) -> anyhow::Result<()> {
            for _ in 0..self.batches {
                let mut t = crate::data::DataTable::empty();
                t.columns = vec![crate::data::ColumnInfo {
                    name: "a".into(),
                    data_type: "Int64".into(),
                }];
                t.rows = vec![vec![crate::data::CellValue::Int(1)]];
                sink(t)?;
            }
            Ok(())
        }
    }

    /// A fake target recording the write mode of each `write_table` call.
    struct FakeTarget {
        modes: Vec<DbWriteMode>,
    }
    impl DbConnector for FakeTarget {
        fn engine(&self) -> DbEngine {
            DbEngine::Snowflake
        }
        fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
        fn query(&mut self, _: &str) -> anyhow::Result<crate::data::DataTable> {
            unreachable!()
        }
        fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        fn write_table(
            &mut self,
            _: Option<&str>,
            _: &str,
            _: &str,
            mode: DbWriteMode,
            data: &crate::data::DataTable,
        ) -> anyhow::Result<DbWriteReport> {
            self.modes.push(mode);
            Ok(DbWriteReport {
                rows_written: data.row_count(),
                created: false,
            })
        }
    }

    fn run_universal_with_fakes(batches: usize, mode: DbWriteMode) -> Vec<DbWriteMode> {
        let mut src = FakeSource { batches };
        let mut tgt = FakeTarget { modes: Vec::new() };
        copy_batches(&mut src, &mut tgt, "SELECT 1", "s", "t", mode).unwrap();
        tgt.modes
    }

    #[test]
    fn universal_lane_creates_then_appends() {
        assert_eq!(
            run_universal_with_fakes(3, DbWriteMode::Replace),
            vec![
                DbWriteMode::Replace,
                DbWriteMode::Append,
                DbWriteMode::Append
            ]
        );
        assert_eq!(
            run_universal_with_fakes(1, DbWriteMode::Create),
            vec![DbWriteMode::Create]
        );
    }

    #[test]
    fn readonly_target_is_refused_before_any_network() {
        let err = copy_table(
            &end(DbEngine::MySql, false),
            None,
            &end(DbEngine::Postgres, false),
            None,
            DbWriteMode::Create,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("Allow writes"), "{err}");
    }

    #[test]
    fn same_table_is_refused() {
        let mut src = end(DbEngine::Postgres, true);
        src.conn.id = "same".into();
        let mut tgt = src.clone();
        tgt.conn.allow_writes = true;
        let err = copy_table(&src, None, &tgt, None, DbWriteMode::Append)
            .unwrap_err()
            .to_string();
        assert!(err.contains("same table"), "{err}");
    }
}
