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
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: super::super::DEFAULT_QUERY_TIMEOUT_SECS,
        tunnel_port: None,
    }
}

fn end(engine: DbEngine, allow_writes: bool) -> DbCopyEnd {
    DbCopyEnd {
        ssh_secret: None,
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
    copy_batches(&mut src, &mut tgt, "SELECT 1", "s", "t", mode, &|_| {}).unwrap();
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
        &|_| {},
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
    let err = copy_table(&src, None, &tgt, None, DbWriteMode::Append, &|_| {})
        .unwrap_err()
        .to_string();
    assert!(err.contains("same table"), "{err}");
}
