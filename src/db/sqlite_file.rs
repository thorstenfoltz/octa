//! A SQLite file as a saved connection, through `rusqlite`. See
//! [`super::file_db`] for the per-action opening every file engine shares.

use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OpenFlags};

use super::DbEngine;
use super::file_db::FileDb;
use crate::data::{ColumnInfo, DataTable, column_type_from_cells};
use crate::formats::sqlite_reader::{sqlite_type_to_arrow, sqlite_value_to_cell};

pub(crate) struct SqliteFile;

impl FileDb for SqliteFile {
    const ENGINE: DbEngine = DbEngine::Sqlite;
    const SCHEMAS_SQL: &'static str =
        "SELECT name FROM pragma_database_list WHERE name <> 'temp' ORDER BY seq";
    type Conn = Connection;

    fn open(path: &Path, read_only: bool) -> Result<Connection> {
        // No SQLITE_OPEN_CREATE: a missing file is refused, never made.
        let access = if read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        let conn = Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        // SQLite reads nothing until a statement needs the schema; asking for
        // its version makes a file that is no database fail here, with "file
        // is not a database", rather than pass Test connection's SELECT 1.
        conn.query_row("PRAGMA schema_version", [], |r| r.get::<_, i64>(0))?;
        Ok(conn)
    }

    fn interrupter(conn: &Connection) -> Box<dyn Fn() + Send> {
        let handle = conn.get_interrupt_handle();
        Box::new(move || handle.interrupt())
    }

    fn query(conn: &Connection, sql: &str) -> Result<DataTable> {
        let mut stmt = conn.prepare(sql)?;
        let names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        let declared: Vec<Option<String>> = stmt
            .columns()
            .iter()
            .map(|c| c.decl_type().map(str::to_string))
            .collect();
        let cap = crate::formats::initial_load_rows();
        let mut rows = Vec::new();
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let row = (0..names.len())
                .map(|i| r.get_ref(i).map(sqlite_value_to_cell))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.push(row);
            if rows.len() >= cap {
                break;
            }
        }
        let columns = names
            .into_iter()
            .zip(declared)
            .enumerate()
            .map(|(i, (name, decl))| ColumnInfo {
                name,
                // An expression has no declared type; its values say what it is.
                data_type: match decl {
                    Some(d) => sqlite_type_to_arrow(&d),
                    None => column_type_from_cells(rows.iter().map(|r| &r[i])).to_string(),
                },
            })
            .collect();
        let mut table = DataTable::empty();
        table.columns = columns;
        table.rows = rows;
        Ok(table)
    }

    fn execute(conn: &Connection, sql: &str) -> Result<u64> {
        Ok(conn.execute(sql, [])? as u64)
    }

    fn tables_sql(schema: &str) -> String {
        format!(
            "SELECT name FROM {}.sqlite_master \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
            DbEngine::Sqlite.quote_ident(schema)
        )
    }
}

#[cfg(test)]
#[path = "sqlite_file_tests.rs"]
mod tests;
