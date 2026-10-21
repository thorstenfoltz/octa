//! A DuckDB file as a saved connection, through the `duckdb` crate. See
//! [`super::file_db`] for the per-action opening every file engine shares.

use std::path::Path;

use anyhow::Result;
use duckdb::{AccessMode, Config, Connection};

use super::DbEngine;
use super::file_db::FileDb;
use crate::data::DataTable;

pub(crate) struct DuckDbFile;

impl FileDb for DuckDbFile {
    const ENGINE: DbEngine = DbEngine::DuckDb;
    const SCHEMAS_SQL: &'static str = "SELECT schema_name FROM information_schema.schemata \
         WHERE catalog_name = current_database() \
           AND schema_name NOT IN ('information_schema', 'pg_catalog') \
         ORDER BY schema_name";
    type Conn = Connection;

    fn open(path: &Path, read_only: bool) -> Result<Connection> {
        let mode = if read_only {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        };
        let conn = Connection::open_with_flags(path, Config::default().access_mode(mode)?)?;
        // A fresh connection reports a TIMESTAMPTZ column as UTC until
        // something has asked for the session zone, and this one is always
        // fresh.
        crate::formats::duckdb_reader::warm_session_timezone(&conn);
        Ok(conn)
    }

    fn interrupter(conn: &Connection) -> Box<dyn Fn() + Send> {
        let handle = conn.interrupt_handle();
        Box::new(move || handle.interrupt())
    }

    fn query(conn: &Connection, sql: &str) -> Result<DataTable> {
        crate::sql::query_table(conn, sql, true)
    }

    fn execute(conn: &Connection, sql: &str) -> Result<u64> {
        Ok(conn.execute(sql, [])? as u64)
    }

    fn tables_sql(schema: &str) -> String {
        format!(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_catalog = current_database() AND table_schema = {} \
             ORDER BY table_name",
            super::relationships::lit(schema)
        )
    }
}

#[cfg(test)]
#[path = "duckdb_file_tests.rs"]
mod tests;
