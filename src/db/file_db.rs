//! What the DuckDB and SQLite file connectors share: the file a saved
//! connection names, and the rule that keeps "open the file per action" safe
//! for writes.
//!
//! Every action opens the file, runs and closes it again, so another program
//! (Python, the DuckDB CLI, a second Octa) can use the file in between: while
//! one process holds a DuckDB file, even read-only, no other process can
//! write it. The exception is a transaction: `BEGIN` keeps its connection
//! until `COMMIT` or `ROLLBACK`, so `write_table_generic`, `apply_write_back`
//! and a user's own `BEGIN ... COMMIT` stay all or nothing.
//! ponytail: `fetch_batches` keeps the trait's LIMIT/OFFSET paging, one open
//! per page; stream from one open if exporting a huge file table is slow.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};

use super::{DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};
use crate::data::DataTable;

/// The file `conn` names (its Database field), `~/` expanded. Refused when
/// the file is not there: opening it would create an empty database, and
/// making one is Save to database's job.
pub fn file_path(conn: &DbConnection) -> Result<PathBuf> {
    let raw = conn.database.trim();
    if raw.is_empty() {
        bail!("connection '{}' names no file", conn.name);
    }
    let path = crate::ssh::expand_home(raw);
    if !path.is_file() {
        bail!("the file {} does not exist", path.display());
    }
    Ok(path)
}

/// What a statement does to a transaction, read off its first words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TxWord {
    /// `BEGIN`, `START TRANSACTION`.
    Begin,
    /// `COMMIT`, `END`, `ROLLBACK`, `ABORT`.
    End,
    Other,
}

pub(crate) fn tx_word(sql: &str) -> TxWord {
    let words: Vec<String> = crate::sql::statement_start(sql)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .take(3)
        .map(str::to_ascii_uppercase)
        .collect();
    match words.first().map(String::as_str) {
        Some("BEGIN" | "START") => TxWord::Begin,
        // `ROLLBACK [TRANSACTION] TO savepoint` stays inside the transaction.
        Some("ROLLBACK") if words.iter().any(|w| w == "TO") => TxWord::Other,
        Some("COMMIT" | "END" | "ROLLBACK" | "ABORT") => TxWord::End,
        _ => TxWord::Other,
    }
}

/// One file engine: how to open its file and run a statement on it.
pub(crate) trait FileDb: 'static {
    const ENGINE: DbEngine;
    /// Schema names, one per row in the first column.
    const SCHEMAS_SQL: &'static str;
    type Conn: Send;
    /// Open `path`, read-only when `read_only`. Never creates the file.
    fn open(path: &Path, read_only: bool) -> Result<Self::Conn>;
    /// Stops the statement running on `conn`, from any thread.
    fn interrupter(conn: &Self::Conn) -> Box<dyn Fn() + Send>;
    fn query(conn: &Self::Conn, sql: &str) -> Result<DataTable>;
    fn execute(conn: &Self::Conn, sql: &str) -> Result<u64>;
    /// The tables and views of `schema`, one per row in the first column.
    fn tables_sql(schema: &str) -> String;
}

/// The interrupt of the statement running now, if one is.
type Running = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// A saved DuckDB or SQLite file behind [`DbConnector`].
pub(crate) struct FileConnector<E: FileDb> {
    path: PathBuf,
    read_only: bool,
    /// The connection `BEGIN` opened, held until the transaction ends.
    tx: Option<E::Conn>,
    running: Running,
}

impl<E: FileDb> FileConnector<E> {
    pub(crate) fn connect(conn: &DbConnection) -> Result<Self> {
        Ok(Self {
            path: file_path(conn)?,
            read_only: !conn.allow_writes,
            tx: None,
            running: Running::default(),
        })
    }

    fn open(&self) -> Result<E::Conn> {
        E::open(&self.path, self.read_only)
            .with_context(|| format!("opening {}", self.path.display()))
    }

    /// `f` on the open transaction's connection, else on the file opened for
    /// this call alone and closed when it returns.
    fn with_conn<T>(&self, f: impl FnOnce(&E::Conn) -> Result<T>) -> Result<T> {
        match &self.tx {
            Some(c) => run::<E, T>(&self.running, c, f),
            None => run::<E, T>(&self.running, &self.open()?, f),
        }
    }
}

/// `f` on `c`, with `c`'s interrupt published for the cancel handle while it
/// runs.
fn run<E: FileDb, T>(
    running: &Running,
    c: &E::Conn,
    f: impl FnOnce(&E::Conn) -> Result<T>,
) -> Result<T> {
    if let Ok(mut r) = running.lock() {
        *r = Some(E::interrupter(c));
    }
    let out = f(c);
    if let Ok(mut r) = running.lock() {
        *r = None;
    }
    out
}

fn first_column(t: &DataTable) -> Vec<String> {
    t.rows
        .iter()
        .filter_map(|r| r.first())
        .map(|v| v.to_string())
        .collect()
}

impl<E: FileDb> DbConnector for FileConnector<E> {
    fn engine(&self) -> DbEngine {
        E::ENGINE
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        super::reject_catalog(E::ENGINE, catalog)?;
        Ok(first_column(&self.query(E::SCHEMAS_SQL)?))
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        super::reject_catalog(E::ENGINE, catalog)?;
        Ok(first_column(&self.query(&E::tables_sql(schema))?))
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        // The SQL panel sends what is not a mutation here, BEGIN and COMMIT
        // included.
        if tx_word(sql) != TxWord::Other {
            self.execute(sql)?;
            return Ok(DataTable::empty());
        }
        self.with_conn(|c| E::query(c, sql))
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        let word = tx_word(sql);
        if word == TxWord::End
            && let Some(c) = self.tx.take()
        {
            // Over whatever the outcome: dropping `c` rolls back what a
            // failed COMMIT left open.
            return run::<E, u64>(&self.running, &c, |c| E::execute(c, sql));
        }
        if word == TxWord::Begin && self.tx.is_none() {
            let c = self.open()?;
            let n = run::<E, u64>(&self.running, &c, |c| E::execute(c, sql))?;
            self.tx = Some(c);
            return Ok(n);
        }
        self.with_conn(|c| E::execute(c, sql))
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        super::reject_catalog(E::ENGINE, catalog)?;
        super::write_table_generic(self, E::ENGINE, None, schema, table, mode, data)
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let running = self.running.clone();
        Some(Box::new(move || {
            if let Ok(r) = running.lock()
                && let Some(stop) = r.as_ref()
            {
                stop();
            }
        }))
    }
}

#[cfg(test)]
#[path = "file_db_tests.rs"]
mod tests;
