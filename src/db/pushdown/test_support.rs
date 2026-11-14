//! A `DbConnector` that answers Postgres-dialect SQL from an in-memory table
//! through DuckDB, which reads the Postgres spellings the pushdown modules
//! send (`PERCENTILE_CONT ... WITHIN GROUP`, `CORR`, `STDDEV_SAMP`, window
//! ranks, double-quoted names). Lets every analysis be checked against the
//! in-memory engine over the same rows, with no server.

use crate::data::DataTable;
use crate::db::{
    DEFAULT_QUERY_TIMEOUT_SECS, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode,
    DbWriteReport,
};

use super::ServerSource;

pub(crate) struct DuckConn {
    table: DataTable,
    /// Further tables, registered under their names beside `data`.
    more: Vec<(&'static str, DataTable)>,
    /// Every statement sent, in order.
    pub(crate) log: Vec<String>,
    /// A statement containing any of these fails, as a server refusal would.
    pub(crate) fail_on: Vec<&'static str>,
}

impl DuckConn {
    pub(crate) fn new(table: DataTable) -> Self {
        Self::with_tables(table, Vec::new())
    }

    /// `main` as `data`, each of `more` under its own name, for analyses
    /// that compare tables.
    pub(crate) fn with_tables(main: DataTable, more: Vec<(&'static str, DataTable)>) -> Self {
        Self {
            table: main,
            more,
            log: Vec::new(),
            fail_on: Vec::new(),
        }
    }
}

/// 0.1, 0.2 and 0.3 in a decimal column `amount`, one group `g`, one value
/// of `k`, one day apart in January, ordered by `id`. A double adds them to
/// 0.6000000000000001; the decimal itself to 0.6.
pub(crate) fn cents() -> DataTable {
    use crate::data::{CellValue, ColumnInfo};
    let mut t = DataTable::empty();
    t.columns = [
        ("id", "Int64"),
        ("g", "Utf8"),
        ("k", "Utf8"),
        ("ts", "Timestamp(Microsecond, None)"),
        ("amount", "Decimal128(2, 1)"),
    ]
    .iter()
    .map(|(n, d)| ColumnInfo {
        name: (*n).into(),
        data_type: (*d).into(),
    })
    .collect();
    t.rows = [0.1, 0.2, 0.3]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            vec![
                CellValue::Int(i as i64 + 1),
                CellValue::String("a".into()),
                CellValue::String("x".into()),
                CellValue::DateTime(format!("2024-01-0{} 10:00:00", i + 1)),
                CellValue::Float(*v),
            ]
        })
        .collect();
    t
}

/// A source whose `from_sql()` is `"<name>"`.
pub(crate) fn source_named(name: &str) -> ServerSource {
    ServerSource {
        table: name.into(),
        ..source()
    }
}

/// A source whose `from_sql()` is `"data"`, the name `run_query` gives the table.
pub(crate) fn source() -> ServerSource {
    ServerSource {
        conn: DbConnection {
            id: "c1".into(),
            name: "test".into(),
            engine: DbEngine::Postgres,
            host: String::new(),
            port: 5432,
            database: String::new(),
            username: String::new(),
            auth: DbAuth::Password,
            allow_writes: false,
            oauth_client_id: None,
            oauth_tenant: None,
            athena_workgroup: None,
            athena_output_location: None,
            datetimeoffset_as_timestamp: false,
            query_timeout_secs: DEFAULT_QUERY_TIMEOUT_SECS,
            ssh: None,
            tls: Default::default(),
            tunnel_port: None,
        },
        catalog: None,
        schema: String::new(),
        table: "data".into(),
        filter: None,
        derived: Vec::new(),
    }
}

impl DbConnector for DuckConn {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        self.log.push(sql.to_string());
        if let Some(f) = self.fail_on.iter().find(|f| sql.contains(*f)) {
            anyhow::bail!("refused: statement contains {f}");
        }
        let sql = sql.replace('`', "\"");
        if self.more.is_empty() {
            return Ok(crate::sql::run_query(&self.table, &sql)?.table);
        }
        let mut ws = crate::sql::SqlWorkspace::new()?;
        ws.set_active_table(&self.table)?;
        for (name, t) in &self.more {
            ws.add_table(name, t, crate::sql::TableOrigin::TabClone((*name).into()))?;
        }
        Ok(ws.execute(&sql)?.table)
    }
    fn engine(&self) -> DbEngine {
        DbEngine::Postgres
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
        anyhow::bail!("read-only test connector")
    }
}

mod tests {
    #[test]
    fn source_named_quotes_the_given_table() {
        assert_eq!(super::source_named("other").from_sql(), "\"other\"");
    }
}
