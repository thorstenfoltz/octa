//! Writing a query result into a DuckDB or SQLite file (Write result to DB).
//! A DuckDB target is ATTACHed and filled with `CREATE TABLE AS` / `INSERT
//! INTO`; a SQLite target goes through `rusqlite`, so the DuckDB `sqlite`
//! extension never has to be present for a write to succeed.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use super::{AttachKind, SqlWorkspace};
use crate::data::{CellValue, DataTable};
use crate::sql::engine::{execute_query, quote_ident};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Error if the target table already exists.
    Create,
    /// Drop and recreate the target table.
    Replace,
    /// Insert rows into an existing target table.
    Append,
}

impl WriteMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "create" => Ok(WriteMode::Create),
            "replace" => Ok(WriteMode::Replace),
            "append" => Ok(WriteMode::Append),
            _ => bail!("unknown write mode '{s}' (use create|replace|append)"),
        }
    }
}

/// What the user wants to write to. Filled by the GUI dialog, the CLI flags,
/// and the MCP `write_to` parameter.
#[derive(Debug, Clone)]
pub struct WriteTarget {
    pub path: PathBuf,
    pub kind: AttachKind,
    pub schema: Option<String>,
    pub table: String,
    pub mode: WriteMode,
    /// The SELECT statement the user wants to persist. The workspace re-runs
    /// it against the current context (the caller must not have mutated the
    /// workspace state between the producing run and the write call).
    pub source_query: String,
    /// Create the target schema if it doesn't already exist (DuckDB only;
    /// SQLite has no schemas).
    pub create_schema_if_missing: bool,
}

#[derive(Debug, Clone)]
pub struct WriteReport {
    pub rows_written: usize,
    pub created_schema: bool,
    pub target_display: String,
}

impl SqlWorkspace {
    /// Write the result of `target.source_query` to a DuckDB or SQLite file.
    /// See [`WriteTarget`] / [`WriteMode`]. Returns the row count actually
    /// written so the caller's UI can show a "Wrote N rows" toast.
    pub fn write_result_to_db(&mut self, target: &WriteTarget) -> Result<WriteReport> {
        match target.kind {
            AttachKind::DuckDb => self.write_to_duckdb(target),
            AttachKind::Sqlite => self.write_to_sqlite(target),
            AttachKind::Postgres | AttachKind::MySql | AttachKind::Mssql => {
                bail!("write-back to a live database goes through db::write_table, not ATTACH")
            }
        }
    }

    fn write_to_duckdb(&mut self, target: &WriteTarget) -> Result<WriteReport> {
        const ATTACH_ALIAS: &str = "__octa_write_target__";
        if self.attachments.contains_key(ATTACH_ALIAS) {
            bail!("internal: write-target alias already attached; refuse to clobber");
        }
        let path_str = target
            .path
            .to_string_lossy()
            .to_string()
            .replace('\'', "''");
        // Auto-create the file if missing: DuckDB's ATTACH does that anyway
        // when given an inexistent path (it creates an empty database file).
        self.conn
            .execute(
                &format!(
                    "ATTACH '{path_str}' AS {} (READ_WRITE)",
                    quote_ident(ATTACH_ALIAS)
                ),
                [],
            )
            .with_context(|| format!("ATTACHing target {}", target.path.display()))?;

        let result = (|| -> Result<WriteReport> {
            let schema = target.schema.as_deref().unwrap_or("main");
            let mut created_schema = false;
            if target.create_schema_if_missing && schema != "main" {
                self.conn
                    .execute(
                        &format!(
                            "CREATE SCHEMA IF NOT EXISTS {}.{}",
                            quote_ident(ATTACH_ALIAS),
                            quote_ident(schema)
                        ),
                        [],
                    )
                    .with_context(|| {
                        format!(
                            "creating target schema {} in {}",
                            schema,
                            target.path.display()
                        )
                    })?;
                created_schema = true;
            }
            let qualified = format!(
                "{}.{}.{}",
                quote_ident(ATTACH_ALIAS),
                quote_ident(schema),
                quote_ident(&target.table)
            );
            let source = target.source_query.trim().trim_end_matches(';');
            let stmt = match target.mode {
                WriteMode::Create => {
                    format!("CREATE TABLE {qualified} AS {source}")
                }
                WriteMode::Replace => {
                    format!("CREATE OR REPLACE TABLE {qualified} AS {source}")
                }
                WriteMode::Append => {
                    format!("INSERT INTO {qualified} {source}")
                }
            };
            // DuckDB returns the affected-row count for INSERT; for CREATE
            // TABLE AS it doesn't, so re-count afterwards.
            let affected = self.conn.execute(&stmt, []).with_context(|| {
                format!(
                    "writing result to {} | {}.{}",
                    target.path.display(),
                    schema,
                    target.table
                )
            })?;
            let rows_written = if matches!(target.mode, WriteMode::Append) {
                affected
            } else {
                self.conn
                    .query_row(&format!("SELECT COUNT(*) FROM {qualified}"), [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .map(|n| n as usize)
                    .unwrap_or(affected)
            };
            let display_schema = target.schema.clone().unwrap_or_else(|| "main".to_string());
            Ok(WriteReport {
                rows_written,
                created_schema,
                target_display: format!(
                    "{} | {}.{}",
                    target.path.display(),
                    display_schema,
                    target.table
                ),
            })
        })();

        // Always DETACH, even on error, so the workspace doesn't leak the
        // target file lock across calls.
        let _ = self
            .conn
            .execute(&format!("DETACH {}", quote_ident(ATTACH_ALIAS)), []);
        result
    }

    fn write_to_sqlite(&mut self, target: &WriteTarget) -> Result<WriteReport> {
        if target
            .schema
            .as_deref()
            .is_some_and(|s| !s.is_empty() && s != "main")
        {
            bail!(
                "SQLite has no schemas: drop the schema or pick `main` (got '{}')",
                target.schema.as_deref().unwrap_or("")
            );
        }
        // Materialise the result via the workspace's DuckDB engine so we
        // honour every registration and attachment the user has set up.
        let source = target.source_query.trim().trim_end_matches(';');
        let result_table = execute_query(&self.conn, source)
            .with_context(|| "running source query for SQLite write-back".to_string())?;

        let mut conn = rusqlite::Connection::open(&target.path)
            .with_context(|| format!("opening SQLite target at {}", target.path.display()))?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
                rusqlite::params![&target.table],
                |r| r.get(0),
            )
            .ok();

        match target.mode {
            WriteMode::Create => {
                if existing.is_some() {
                    bail!(
                        "table '{}' already exists; choose Replace or Append",
                        target.table
                    );
                }
            }
            WriteMode::Replace => {
                if existing.is_some() {
                    conn.execute(
                        &format!("DROP TABLE {}", quote_ident_sqlite(&target.table)),
                        [],
                    )?;
                }
            }
            WriteMode::Append => {
                if existing.is_none() {
                    bail!(
                        "table '{}' does not exist; choose Create or Replace",
                        target.table
                    );
                }
            }
        }

        let rows_written = if matches!(target.mode, WriteMode::Append) {
            insert_into_sqlite(&mut conn, &target.table, &result_table)?
        } else {
            create_sqlite_table(&mut conn, &target.table, &result_table)?;
            insert_into_sqlite(&mut conn, &target.table, &result_table)?
        };

        Ok(WriteReport {
            rows_written,
            created_schema: false,
            target_display: format!("{} | {}", target.path.display(), target.table),
        })
    }
}

// --- SQLite write helpers ---

fn quote_ident_sqlite(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn create_sqlite_table(
    conn: &mut rusqlite::Connection,
    table: &str,
    data: &DataTable,
) -> Result<()> {
    let cols_sql: Vec<String> = data
        .columns
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                quote_ident_sqlite(&c.name),
                arrow_to_sqlite_type(&c.data_type)
            )
        })
        .collect();
    let create = format!(
        "CREATE TABLE {} ({})",
        quote_ident_sqlite(table),
        cols_sql.join(", ")
    );
    conn.execute(&create, [])?;
    Ok(())
}

fn insert_into_sqlite(
    conn: &mut rusqlite::Connection,
    table: &str,
    data: &DataTable,
) -> Result<usize> {
    if data.col_count() == 0 || data.row_count() == 0 {
        return Ok(0);
    }
    let col_idents = data
        .columns
        .iter()
        .map(|c| quote_ident_sqlite(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (0..data.col_count())
        .map(|_| "?".to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident_sqlite(table),
        col_idents,
        placeholders
    );
    let tx = conn.transaction()?;
    let mut written = 0usize;
    {
        let mut stmt = tx.prepare(&sql)?;
        for row_idx in 0..data.row_count() {
            let params: Vec<rusqlite::types::Value> = (0..data.col_count())
                .map(|c| cell_to_sqlite_value(data.get(row_idx, c).unwrap_or(&CellValue::Null)))
                .collect();
            stmt.execute(rusqlite::params_from_iter(params))?;
            written += 1;
        }
    }
    tx.commit()?;
    Ok(written)
}

fn arrow_to_sqlite_type(arrow_ty: &str) -> &'static str {
    match arrow_ty {
        "Int64" | "Int32" | "Int16" | "Int8" | "Boolean" => "INTEGER",
        "Float64" | "Float32" => "REAL",
        "Binary" | "LargeBinary" => "BLOB",
        _ => "TEXT",
    }
}

fn cell_to_sqlite_value(v: &CellValue) -> rusqlite::types::Value {
    use rusqlite::types::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Integer(*b as i64),
        CellValue::Int(n) => Value::Integer(*n),
        CellValue::Float(f) => Value::Real(*f),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => Value::Text(s.clone()),
        CellValue::Time(_) | CellValue::Duration(_) => Value::Text(v.to_string()),
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}
