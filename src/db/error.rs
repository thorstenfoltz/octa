//! Who stopped a database action, said in front of its error.
//!
//! An error the database returned, a conversation that broke off and Octa's
//! own refusal (see [`super::ensure_write_allowed`]) used to read alike, so
//! nobody could tell whether to fix the statement, the network or a setting.
//! Every connector [`super::connect`] opens is wrapped here, so each of its
//! errors starts with one sentence naming the source:
//!
//! - "Databricks returned an error." for what the server itself sent,
//! - "Octa could not finish talking to 'name'." (or "could not connect to")
//!   for everything else: the network, a timeout, a cancel, an answer Octa
//!   could not read.
//!
//! The sentence ends in a line break rather than anyhow's ": ", which keeps
//! a colon out of the text. The cost is that the labelled error carries the
//! rest as text, so it cannot be downcast; nothing downcasts a connector's
//! error, and code that must see one (the identifier fix) reads the text,
//! which is all still there.

use std::fmt;

use super::{DbConnector, DbEngine, DbWriteMode, DbWriteReport};
use crate::data::DataTable;

/// An error in the server's own words, for the engines Octa talks to over
/// HTTP itself. The wire drivers have error types of their own, recognised
/// in [`from_server`].
#[derive(Debug)]
pub struct ServerError(pub String);

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ServerError {}

/// Whether the server itself sent `e` (as opposed to the network, a timeout,
/// a cancel or Octa failing to read the answer).
fn from_server(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.is::<ServerError>()
            || c.downcast_ref::<tokio_postgres::Error>()
                .is_some_and(|e| e.as_db_error().is_some())
            || matches!(
                c.downcast_ref::<mysql_async::Error>(),
                Some(mysql_async::Error::Server(_))
            )
            || matches!(
                c.downcast_ref::<tiberius::error::Error>(),
                Some(tiberius::error::Error::Server(_))
            )
            || matches!(
                c.downcast_ref::<oracle_rs::Error>(),
                Some(oracle_rs::Error::OracleError { .. })
            )
            || matches!(
                c.downcast_ref::<sqlx_exasol::Error>(),
                Some(sqlx_exasol::Error::Database(_))
            )
            || matches!(
                c.downcast_ref::<duckdb::Error>(),
                Some(duckdb::Error::DuckDBFailure(..))
            )
            || matches!(
                c.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(..) | rusqlite::Error::SqlInputError { .. })
            )
    })
}

/// An error with its source sentence in front; see the module docs.
#[derive(Debug)]
struct Labelled(String);

impl fmt::Display for Labelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Labelled {}

/// An error the receiving side of [`DbConnector::fetch_batches`] raised (an
/// export writing a file), carried through the connector untouched: it is
/// neither the server's doing nor the connection's.
struct FromSink(anyhow::Error);

impl fmt::Debug for FromSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl fmt::Display for FromSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.0)
    }
}

impl std::error::Error for FromSink {}

/// `e` led by who stopped it. `failed` is what Octa says when the server did
/// not: "could not connect to 'name'" or "could not finish talking to 'name'".
fn label(engine: DbEngine, failed: &str, e: anyhow::Error) -> anyhow::Error {
    let lead = if from_server(&e) {
        format!("{} returned an error.", engine.label())
    } else {
        format!("Octa {failed}.")
    };
    anyhow::Error::new(Labelled(format!("{lead}\n{e:#}")))
}

/// A connect failure, led by who stopped it.
pub(super) fn label_connect(engine: DbEngine, name: &str, e: anyhow::Error) -> anyhow::Error {
    label(engine, &format!("could not connect to '{name}'"), e)
}

/// `inner` with every error led by who stopped it.
pub(super) fn labelled(inner: Box<dyn DbConnector>, name: &str) -> Box<dyn DbConnector> {
    Box::new(LabelledConnector {
        failed: format!("could not finish talking to '{name}'"),
        inner,
    })
}

struct LabelledConnector {
    inner: Box<dyn DbConnector>,
    failed: String,
}

impl LabelledConnector {
    fn label<T>(&self, r: anyhow::Result<T>) -> anyhow::Result<T> {
        r.map_err(|e| label(self.inner.engine(), &self.failed, e))
    }
}

/// Every method, defaults included: a default left out here would run the
/// trait's version instead of the connector's own (Databricks pages through
/// external links in its own `fetch_batches`).
impl DbConnector for LabelledConnector {
    fn list_catalogs(&mut self) -> anyhow::Result<Vec<String>> {
        let r = self.inner.list_catalogs();
        self.label(r)
    }
    fn list_schemas(&mut self, catalog: Option<&str>) -> anyhow::Result<Vec<String>> {
        let r = self.inner.list_schemas(catalog);
        self.label(r)
    }
    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> anyhow::Result<Vec<String>> {
        let r = self.inner.list_tables(catalog, schema);
        self.label(r)
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        let r = self.inner.query(sql);
        self.label(r)
    }
    fn engine(&self) -> DbEngine {
        self.inner.engine()
    }
    fn fetch_batches(
        &mut self,
        sql: &str,
        batch_rows: usize,
        sink: &mut dyn FnMut(DataTable) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut sink = |batch| sink(batch).map_err(|e| anyhow::Error::new(FromSink(e)));
        match self.inner.fetch_batches(sql, batch_rows, &mut sink) {
            Ok(()) => Ok(()),
            Err(e) => match e.downcast::<FromSink>() {
                Ok(FromSink(e)) => Err(e),
                Err(e) => self.label(Err(e)),
            },
        }
    }
    fn execute(&mut self, sql: &str) -> anyhow::Result<u64> {
        let r = self.inner.execute(sql);
        self.label(r)
    }
    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        let r = self.inner.write_table(catalog, schema, table, mode, data);
        self.label(r)
    }
    fn check_rows_writable(
        &self,
        columns: &[crate::data::ColumnInfo],
        rows: &[&[crate::data::CellValue]],
    ) -> anyhow::Result<()> {
        self.inner.check_rows_writable(columns, rows)
    }
    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        self.inner.cancel_handle()
    }
    fn dry_run_bytes(&mut self, sql: &str) -> anyhow::Result<u64> {
        let r = self.inner.dry_run_bytes(sql);
        self.label(r)
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
