//! Persistent multi-table SQL workspace.
//!
//! Today's `run_query` is a one-shot affair: open a fresh in-memory DuckDB
//! connection, register the caller's `DataTable` as the temp table `data`,
//! execute one statement, tear the connection down. That model has no place
//! for JOINs across multiple sources, ATTACH-ed databases, schema-qualified
//! queries, or write-back to a real DB file.
//!
//! `SqlWorkspace` keeps the same DuckDB connection alive for the lifetime of
//! the workspace and exposes a small mutation surface:
//!
//! - [`SqlWorkspace::set_active_table`] (re)registers the caller's
//!   `DataTable` as `data` (or whatever name the caller chose).
//! - [`SqlWorkspace::add_table_from_file`] loads any supported format via
//!   [`crate::formats::FormatRegistry`] and registers it under a SQL
//!   identifier so JOINs across heterogeneous sources work natively.
//! - [`SqlWorkspace::attach`] runs `ATTACH 'file' AS alias` for DuckDB
//!   files (and, when the bundled DuckDB ships the `sqlite` extension, for
//!   SQLite files too; otherwise the workspace falls back to per-table
//!   loading via rusqlite).
//! - [`SqlWorkspace::execute`] runs a query against the assembled context
//!   and returns the same [`QueryOutcome`] shape as today's `run_query`.
//! - [`SqlWorkspace::write_result_to_db`] writes the result of a SELECT
//!   back into a DuckDB or SQLite file (target schema + table + mode). For
//!   DuckDB targets the workspace ATTACHes the file and runs
//!   `CREATE TABLE AS` / `INSERT INTO`; for SQLite targets it materialises
//!   the result and uses `rusqlite` directly so the DuckDB SQLite extension
//!   never has to be present for writes to succeed.
//!
//! `src/sql/mod.rs::run_query` is now a one-line delegator over this
//! workspace, so every existing caller (GUI legacy path, CLI single-file
//! `--sql`, MCP `run_sql` single-file mode) keeps working unchanged.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use duckdb::Connection;

use crate::data::{CellValue, DataTable};
use crate::formats::FormatRegistry;

use super::engine::{
    QueryKind, QueryOutcome, execute_query, h2o_easter_egg, is_mutation, octopuses_easter_egg,
    quote_ident, register_table_into, stars_easter_egg,
};

/// Origin of a registered workspace table. Recorded so the panel and the
/// MCP / CLI surfaces can show users where each table came from.
#[derive(Debug, Clone)]
pub enum TableOrigin {
    /// Current tab's active table (always registered, conventionally as `data`).
    ActiveTab,
    /// File loaded via [`FormatRegistry`]. `inner_table` is set for multi-table
    /// sources (DuckDB / SQLite / Excel / ODS).
    File {
        path: PathBuf,
        inner_table: Option<String>,
    },
    /// In-memory clone of another tab. The string is the source description
    /// shown in the panel (file stem or "untitled").
    TabClone(String),
}

impl TableOrigin {
    pub fn display(&self) -> String {
        match self {
            TableOrigin::ActiveTab => "active tab".to_string(),
            TableOrigin::File { path, inner_table } => {
                let stem = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                match inner_table {
                    Some(t) => format!("{stem} | {t}"),
                    None => stem,
                }
            }
            TableOrigin::TabClone(label) => format!("tab: {label}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredTable {
    pub sql_name: String,
    pub origin: TableOrigin,
    pub row_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachKind {
    DuckDb,
    Sqlite,
}

impl AttachKind {
    pub fn from_path(path: &Path) -> AttachKind {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "duckdb" | "ddb" => AttachKind::DuckDb,
            _ => AttachKind::Sqlite,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub alias: String,
    pub path: PathBuf,
    pub kind: AttachKind,
    /// `true` when the attachment is a real DuckDB `ATTACH`; `false` when the
    /// workspace fell back to per-table loading for a SQLite file because the
    /// DuckDB `sqlite` extension wasn't available.
    pub native: bool,
}

#[derive(Debug, Clone)]
pub struct AttachedTable {
    pub schema: String,
    pub table: String,
    pub row_count: Option<usize>,
}

/// Single-column entry in a [`TableInspection`]: name + DuckDB-formatted type.
#[derive(Debug, Clone)]
pub struct ColumnInspection {
    pub name: String,
    pub data_type: String,
}

/// One-shot snapshot of a workspace or attached table: qualified name,
/// row count, column list, and a small row sample. Produced on demand by
/// [`SqlWorkspace::inspect_registered_table`] and
/// [`SqlWorkspace::inspect_attached_table`] so the UI can show users what's
/// inside an attachment without re-running queries every frame.
#[derive(Debug, Clone)]
pub struct TableInspection {
    /// Fully qualified name the user would type in SQL (e.g. `customers`,
    /// `wh.main.orders`). The inspector copies / inserts this verbatim.
    pub qualified_name: String,
    pub row_count: Option<usize>,
    pub columns: Vec<ColumnInspection>,
    /// First N rows as displayable strings; one row per outer Vec.
    pub sample_rows: Vec<Vec<String>>,
}

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

/// Cross-format SQL workspace. See the module docstring for the model.
pub struct SqlWorkspace {
    conn: Connection,
    tables: BTreeMap<String, RegisteredTable>,
    attachments: BTreeMap<String, Attachment>,
    /// `Some(true)` once `INSTALL sqlite; LOAD sqlite;` has succeeded;
    /// `Some(false)` once it has failed (we skip retrying); `None` until the
    /// first SQLite attach attempt.
    sqlite_extension: Option<bool>,
}

impl SqlWorkspace {
    pub fn new() -> Result<Self> {
        Ok(Self {
            conn: Connection::open_in_memory().context("opening in-memory DuckDB")?,
            tables: BTreeMap::new(),
            attachments: BTreeMap::new(),
            sqlite_extension: None,
        })
    }

    /// Register or replace the conventional `data` table. The caller passes
    /// the active tab's `DataTable`; we drop any prior `data` registration
    /// and recreate it. This is the single-table fast path that mirrors the
    /// old `run_query` behaviour.
    pub fn set_active_table(&mut self, table: &DataTable) -> Result<()> {
        self.register_or_replace("data", table, TableOrigin::ActiveTab)
    }

    /// Register a `DataTable` under `sql_name`. Returns the row count for
    /// the caller's UI. Replaces any existing registration with the same name.
    pub fn add_table(
        &mut self,
        sql_name: &str,
        table: &DataTable,
        origin: TableOrigin,
    ) -> Result<RegisteredTable> {
        self.register_or_replace(sql_name, table, origin.clone())?;
        Ok(self
            .tables
            .get(sql_name)
            .cloned()
            .unwrap_or(RegisteredTable {
                sql_name: sql_name.to_string(),
                origin,
                row_count: table.row_count(),
            }))
    }

    /// Read `path` via [`FormatRegistry`] and register the result under
    /// `sql_name`. For multi-table sources `inner_table` picks which inner
    /// table to load (qualified `schema.table` form for DuckDB).
    pub fn add_table_from_file(
        &mut self,
        path: &Path,
        inner_table: Option<&str>,
        sql_name: &str,
    ) -> Result<RegisteredTable> {
        let registry = FormatRegistry::new();
        let reader = registry
            .reader_for_path(path)
            .ok_or_else(|| anyhow!("no reader for {}", path.display()))?;
        let table = match inner_table {
            Some(t) => reader.read_table(path, t)?,
            None => reader.read_file(path)?,
        };
        let origin = TableOrigin::File {
            path: path.to_path_buf(),
            inner_table: inner_table.map(|s| s.to_string()),
        };
        self.add_table(sql_name, &table, origin)
    }

    pub fn remove_table(&mut self, sql_name: &str) -> Result<()> {
        if self.tables.remove(sql_name).is_none() {
            bail!("no table named '{sql_name}' in workspace");
        }
        self.conn
            .execute(
                &format!("DROP TABLE IF EXISTS {}", quote_ident(sql_name)),
                [],
            )
            .with_context(|| format!("dropping temp table {sql_name}"))?;
        Ok(())
    }

    /// ATTACH a DuckDB or SQLite database under `alias` so its tables are
    /// addressable as `alias.schema.table`. For SQLite the workspace tries
    /// the DuckDB `sqlite` extension first; if `INSTALL sqlite; LOAD sqlite;`
    /// fails the workspace falls back to enumerating the file's tables via
    /// `rusqlite` and registering each as a normal workspace table under the
    /// name `alias__table`. The boolean `native` field on the returned
    /// [`Attachment`] tells the caller which path was taken.
    pub fn attach(&mut self, path: &Path, alias: &str, kind: AttachKind) -> Result<Attachment> {
        if self.attachments.contains_key(alias) {
            bail!("alias '{alias}' is already attached");
        }
        let path_str = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned();
        match kind {
            AttachKind::DuckDb => {
                self.conn
                    .execute(
                        &format!(
                            "ATTACH '{}' AS {} (READ_ONLY)",
                            path_str.replace('\'', "''"),
                            quote_ident(alias)
                        ),
                        [],
                    )
                    .with_context(|| format!("ATTACHing DuckDB at {}", path.display()))?;
                let attachment = Attachment {
                    alias: alias.to_string(),
                    path: path.to_path_buf(),
                    kind,
                    native: true,
                };
                self.attachments
                    .insert(alias.to_string(), attachment.clone());
                Ok(attachment)
            }
            AttachKind::Sqlite => {
                if self.ensure_sqlite_extension() {
                    let attach_sql = format!(
                        "ATTACH '{}' AS {} (TYPE SQLITE, READ_ONLY)",
                        path_str.replace('\'', "''"),
                        quote_ident(alias)
                    );
                    if self.conn.execute(&attach_sql, []).is_ok() {
                        let attachment = Attachment {
                            alias: alias.to_string(),
                            path: path.to_path_buf(),
                            kind,
                            native: true,
                        };
                        self.attachments
                            .insert(alias.to_string(), attachment.clone());
                        return Ok(attachment);
                    }
                }
                // Fallback: enumerate tables via rusqlite and register each
                // as a workspace table under `alias__table`. The user loses
                // the schema-qualified addressing but gets the same JOIN
                // capability on every install.
                self.attach_sqlite_fallback(path, alias)
            }
        }
    }

    pub fn detach(&mut self, alias: &str) -> Result<()> {
        let attachment = self
            .attachments
            .remove(alias)
            .ok_or_else(|| anyhow!("no attachment '{alias}'"))?;
        if attachment.native {
            self.conn
                .execute(&format!("DETACH {}", quote_ident(alias)), [])
                .with_context(|| format!("DETACHing {alias}"))?;
        } else {
            // Fallback registrations carried the alias prefix; drop them all.
            let prefix = format!("{alias}__");
            let keys: Vec<String> = self
                .tables
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            for k in keys {
                let _ = self.remove_table(&k);
            }
        }
        Ok(())
    }

    pub fn list_tables(&self) -> Vec<&RegisteredTable> {
        self.tables.values().collect()
    }

    pub fn list_attached(&self) -> Vec<&Attachment> {
        self.attachments.values().collect()
    }

    /// Run `sql` and push the first column of every returned row into `out`,
    /// silently ignoring failures. Used by
    /// [`SqlWorkspace::collect_autocomplete_identifiers`] so the editor's
    /// autocomplete keeps working even if `information_schema` queries fail
    /// after a corrupt attachment.
    fn push_first_column(conn: &Connection, sql: &str, out: &mut Vec<String>) {
        let Ok(mut stmt) = conn.prepare(sql) else {
            return;
        };
        let Ok(mut rows) = stmt.query([]) else {
            return;
        };
        while let Ok(Some(r)) = rows.next() {
            if let Ok(name) = r.get::<_, String>(0) {
                out.push(name);
            }
        }
    }

    /// Flat list of identifiers visible to the SQL editor's autocomplete:
    /// registered workspace table names, attachment aliases, table names in
    /// every attached database, and column names from every visible table.
    /// Sorted, deduplicated. One `information_schema` round-trip - cheap
    /// enough to call per frame for typical workspaces.
    pub fn collect_autocomplete_identifiers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for alias in self.attachments.keys() {
            out.push(alias.clone());
        }
        for name in self.tables.keys() {
            out.push(name.clone());
        }
        Self::push_first_column(
            &self.conn,
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog')",
            &mut out,
        );
        Self::push_first_column(
            &self.conn,
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog')",
            &mut out,
        );
        out.sort();
        out.dedup();
        out
    }

    /// Enumerate tables inside an attached DuckDB or SQLite database via the
    /// live DuckDB connection's `information_schema`. Returns an empty Vec
    /// for fallback attachments (the user already sees those tables as
    /// regular workspace entries).
    pub fn list_attached_tables(&self, alias: &str) -> Result<Vec<AttachedTable>> {
        let attachment = self
            .attachments
            .get(alias)
            .ok_or_else(|| anyhow!("no attachment '{alias}'"))?;
        if !attachment.native {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT table_schema, table_name FROM information_schema.tables \
             WHERE table_catalog = ? AND table_type = 'BASE TABLE' \
             ORDER BY table_schema, table_name",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map([alias], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for (schema, table) in rows {
            let count_sql = format!(
                "SELECT COUNT(*) FROM {}.{}.{}",
                quote_ident(alias),
                quote_ident(&schema),
                quote_ident(&table)
            );
            let row_count: Option<usize> = self
                .conn
                .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
                .ok()
                .map(|n| n as usize);
            out.push(AttachedTable {
                schema,
                table,
                row_count,
            });
        }
        Ok(out)
    }

    /// Inspect a registered workspace table by its SQL name. Returns column
    /// list, row count, and up to `sample_rows` rows formatted as strings.
    /// Errors propagate as Err; the panel surfaces the message inline.
    pub fn inspect_registered_table(
        &self,
        sql_name: &str,
        sample_rows: usize,
    ) -> Result<TableInspection> {
        if !self.tables.contains_key(sql_name) {
            bail!("no registered workspace table '{sql_name}'");
        }
        self.inspect_by_qualified_name(sql_name, sql_name, sample_rows)
    }

    /// Inspect a table inside an ATTACH-ed (native) database. `schema` and
    /// `table` are unquoted; this function quotes them safely. Fallback
    /// attachments aren't supported here because their tables are already
    /// surfaced as regular workspace entries (use
    /// [`inspect_registered_table`] for those).
    pub fn inspect_attached_table(
        &self,
        alias: &str,
        schema: &str,
        table: &str,
        sample_rows: usize,
    ) -> Result<TableInspection> {
        let attachment = self
            .attachments
            .get(alias)
            .ok_or_else(|| anyhow!("no attachment '{alias}'"))?;
        if !attachment.native {
            bail!(
                "attachment '{alias}' is a fallback-loaded SQLite; inspect via the registered \
                 workspace table instead"
            );
        }
        let qualified = format!(
            "{}.{}.{}",
            quote_ident(alias),
            quote_ident(schema),
            quote_ident(table)
        );
        let display = format!("{alias}.{schema}.{table}");
        self.inspect_by_qualified_name(&qualified, &display, sample_rows)
    }

    fn inspect_by_qualified_name(
        &self,
        qualified_sql: &str,
        display_name: &str,
        sample_rows: usize,
    ) -> Result<TableInspection> {
        let mut columns: Vec<ColumnInspection> = Vec::new();
        let describe_sql = format!("DESCRIBE {qualified_sql}");
        let mut stmt = self
            .conn
            .prepare(&describe_sql)
            .with_context(|| format!("DESCRIBE {display_name}"))?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            let name: String = r.get(0)?;
            let data_type: String = r.get(1)?;
            columns.push(ColumnInspection { name, data_type });
        }
        drop(rows);
        drop(stmt);

        let count_sql = format!("SELECT COUNT(*) FROM {qualified_sql}");
        let row_count: Option<usize> = self
            .conn
            .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
            .ok()
            .map(|n| n as usize);

        let mut sample: Vec<Vec<String>> = Vec::new();
        if sample_rows > 0 {
            let preview_sql = format!("SELECT * FROM {qualified_sql} LIMIT {sample_rows}");
            if let Ok(table) = execute_query(&self.conn, &preview_sql) {
                for r in 0..table.row_count() {
                    let mut row: Vec<String> = Vec::with_capacity(table.col_count());
                    for c in 0..table.col_count() {
                        row.push(table.get(r, c).map(|v| v.to_string()).unwrap_or_default());
                    }
                    sample.push(row);
                }
            }
        }

        Ok(TableInspection {
            qualified_name: display_name.to_string(),
            row_count,
            columns,
            sample_rows: sample,
        })
    }

    /// Execute a statement against the workspace's persistent connection.
    /// Same shape as `run_query`. Mutations re-select `data` (preserves the
    /// existing single-table behaviour for the GUI's mutation flow).
    pub fn execute(&mut self, query: &str) -> Result<QueryOutcome> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            bail!("Query is empty");
        }
        if let Some(egg) = octopuses_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
            });
        }
        if let Some(egg) = stars_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
            });
        }
        if let Some(egg) = h2o_easter_egg(trimmed) {
            return Ok(QueryOutcome {
                kind: QueryKind::Select,
                affected: None,
                table: egg,
            });
        }
        if is_mutation(trimmed) {
            let affected = self.conn.execute(trimmed, [])?;
            // Re-select `data` so the GUI mutation path sees the post-state.
            // If `data` no longer exists (the user dropped it), return an
            // empty result rather than erroring.
            let mutated =
                execute_query(&self.conn, "SELECT * FROM data").unwrap_or_else(|_| DataTable {
                    columns: Vec::new(),
                    rows: Vec::new(),
                    edits: HashMap::new(),
                    source_path: None,
                    format_name: Some("SQL Result".to_string()),
                    structural_changes: false,
                    total_rows: None,
                    row_offset: 0,
                    marks: HashMap::new(),
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
                    db_meta: None,
                });
            return Ok(QueryOutcome {
                kind: QueryKind::Mutation,
                affected: Some(affected),
                table: mutated,
            });
        }
        let result = execute_query(&self.conn, trimmed)?;
        Ok(QueryOutcome {
            kind: QueryKind::Select,
            affected: None,
            table: result,
        })
    }

    /// Write the result of `target.source_query` to a DuckDB or SQLite file.
    /// See [`WriteTarget`] / [`WriteMode`]. Returns the row count actually
    /// written so the caller's UI can show a "Wrote N rows" toast.
    pub fn write_result_to_db(&mut self, target: &WriteTarget) -> Result<WriteReport> {
        match target.kind {
            AttachKind::DuckDb => self.write_to_duckdb(target),
            AttachKind::Sqlite => self.write_to_sqlite(target),
        }
    }

    // --- internal helpers ---

    fn register_or_replace(
        &mut self,
        sql_name: &str,
        table: &DataTable,
        origin: TableOrigin,
    ) -> Result<()> {
        // DROP first so re-registering `data` after an edit doesn't error.
        self.conn
            .execute(
                &format!("DROP TABLE IF EXISTS {}", quote_ident(sql_name)),
                [],
            )
            .with_context(|| format!("dropping temp table {sql_name}"))?;
        register_table_into(&self.conn, sql_name, table)?;
        self.tables.insert(
            sql_name.to_string(),
            RegisteredTable {
                sql_name: sql_name.to_string(),
                origin,
                row_count: table.row_count(),
            },
        );
        Ok(())
    }

    fn ensure_sqlite_extension(&mut self) -> bool {
        if let Some(loaded) = self.sqlite_extension {
            return loaded;
        }
        let ok = self
            .conn
            .execute_batch("INSTALL sqlite; LOAD sqlite;")
            .is_ok();
        self.sqlite_extension = Some(ok);
        ok
    }

    fn attach_sqlite_fallback(&mut self, path: &Path, alias: &str) -> Result<Attachment> {
        let registry = FormatRegistry::new();
        let reader = registry
            .reader_for_path(path)
            .ok_or_else(|| anyhow!("no SQLite reader available"))?;
        let listing = reader.list_tables(path)?.unwrap_or_default();
        for info in &listing {
            let sql_name = format!("{}__{}", alias, info.name);
            // Best-effort: skip tables that fail to load rather than blowing
            // up the whole attach.
            if let Ok(t) = reader.read_table(path, &info.name) {
                let origin = TableOrigin::File {
                    path: path.to_path_buf(),
                    inner_table: Some(info.name.clone()),
                };
                let _ = self.add_table(&sql_name, &t, origin);
            }
        }
        let attachment = Attachment {
            alias: alias.to_string(),
            path: path.to_path_buf(),
            kind: AttachKind::Sqlite,
            native: false,
        };
        self.attachments
            .insert(alias.to_string(), attachment.clone());
        Ok(attachment)
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

/// Build a unique SQL name from `base` by appending `_2`, `_3`, ... until it
/// no longer collides with `existing`. Caller passes the registry's keys.
pub fn dedupe_sql_name<F: Fn(&str) -> bool>(base: &str, exists: F) -> String {
    let base = sanitize_sql_name(base);
    if !exists(&base) {
        return base;
    }
    let mut i = 2;
    loop {
        let candidate = format!("{base}_{i}");
        if !exists(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// Sanitise a free-form string into a friendly SQL identifier. Lowercases,
/// replaces non-alphanumeric (and non-underscore) chars with `_`, and prefixes
/// with `t_` if the result starts with a digit. Empty input becomes `table`.
pub fn sanitize_sql_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        return "table".to_string();
    }
    if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("t_{trimmed}")
    } else {
        trimmed
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
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
