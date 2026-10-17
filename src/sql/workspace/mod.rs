//! Persistent multi-table SQL workspace.
//!
//! `run_query` is a one-shot affair: open a fresh in-memory DuckDB
//! connection, register the caller's `DataTable` as the table `data`,
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use duckdb::Connection;

use crate::data::DataTable;
use crate::formats::FormatRegistry;

use super::engine::{QueryOutcome, execute_query, quote_ident, register_table_into};

mod attach;
mod session;
mod write;

pub use attach::{
    AttachKind, AttachOutcome, AttachScope, AttachedTable, Attachment, FileAttached, ImportSink,
    ImportedTable, duckdb_attach_sql, import_scope,
};
use session::Session;
pub use session::{DataFeed, JobQuery, PagedResult, RunDone};
pub use write::{WriteMode, WriteReport, WriteTarget};

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
    /// Snapshot imported from a live database connection (the SQL Server
    /// attach fallback). The string is "conn-name schema.table".
    Db(String),
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
            TableOrigin::Db(label) => format!("db: {label}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredTable {
    pub sql_name: String,
    pub origin: TableOrigin,
    pub row_count: usize,
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

/// Cross-format SQL workspace. See the module docstring for the model.
pub struct SqlWorkspace {
    conn: Connection,
    tables: BTreeMap<String, RegisteredTable>,
    attachments: BTreeMap<String, Attachment>,
    /// Per-alias cache of [`Self::list_attached_tables`] (interior mutability
    /// so the `&self` listing keeps its signature). See that method for why
    /// this must exist.
    attached_tables_cache: std::cell::RefCell<BTreeMap<String, Vec<AttachedTable>>>,
    /// `Some(true)` once `INSTALL sqlite; LOAD sqlite;` has succeeded;
    /// `Some(false)` once it has failed (we skip retrying); `None` until the
    /// first SQLite attach attempt.
    sqlite_extension: Option<bool>,
    /// A second connection to the same database, for [`Self::run_job`]: a
    /// worker holds it while a query runs or `data` is copied, and the
    /// window keeps `conn`. Kept rather than cloned per run, so a `SET` or a
    /// macro from one run is still there for the next.
    runner: Arc<Mutex<Connection>>,
}

impl SqlWorkspace {
    pub fn new() -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
        crate::formats::duckdb_reader::warm_session_timezone(&conn);
        let runner = conn
            .try_clone()
            .context("opening a second DuckDB connection")?;
        crate::formats::duckdb_reader::warm_session_timezone(&runner);
        Ok(Self {
            conn,
            tables: BTreeMap::new(),
            attachments: BTreeMap::new(),
            attached_tables_cache: std::cell::RefCell::new(BTreeMap::new()),
            sqlite_extension: None,
            runner: Arc::new(Mutex::new(runner)),
        })
    }

    /// A handle that interrupts whatever the runner connection is running,
    /// for a job's Cancel.
    pub fn runner_interrupt(&self) -> Result<Arc<duckdb::InterruptHandle>> {
        Ok(self
            .runner
            .lock()
            .map_err(|_| anyhow!("the SQL worker stopped"))?
            .interrupt_handle())
    }

    /// List `data` as the active tab's, `rows` long, after a
    /// [`Self::run_job`] copied it.
    pub fn note_active_table(&mut self, rows: usize) {
        self.note_table("data", TableOrigin::ActiveTab, rows);
    }

    /// List `name`, `rows` long, as a workspace table after a worker copied
    /// it (see [`Self::copy_tables_job`]).
    pub fn note_table(&mut self, name: &str, origin: TableOrigin, rows: usize) {
        self.tables.insert(
            name.to_string(),
            RegisteredTable {
                sql_name: name.to_string(),
                origin,
                row_count: rows,
            },
        );
    }

    /// Register or replace the conventional `data` table. The caller passes
    /// the active tab's `DataTable`; we drop any prior `data` registration
    /// and recreate it. This is the single-table fast path that mirrors the
    /// old `run_query` behaviour.
    pub fn set_active_table(&mut self, table: &DataTable) -> Result<()> {
        self.register_or_replace("data", table, TableOrigin::ActiveTab)
    }

    /// Register `data` as a view that scans `path` in place, for a tab whose
    /// rows stay on disk (large-file mode): its table is only the page on
    /// screen, and copying that would answer every query about 2,000 rows of
    /// a file of millions. Listed like any active table, with the file's
    /// `row_count`, so the inspector never has to count the file.
    pub fn set_active_scan(&mut self, path: &Path, row_count: usize) -> Result<()> {
        self.drop_relation("data")?;
        self.add_view_from_scan("data", path)?;
        self.tables.insert(
            "data".to_string(),
            RegisteredTable {
                sql_name: "data".to_string(),
                origin: TableOrigin::ActiveTab,
                row_count,
            },
        );
        Ok(())
    }

    /// Register a file as a DuckDB **view** that scans it in place, so a
    /// query never materialises its rows. Only the formats DuckDB can scan
    /// qualify; the caller checks that with
    /// [`ScanKind::for_path`](crate::formats::large::ScanKind::for_path).
    ///
    /// This is what makes `--sql --stream` able to aggregate a file far larger
    /// than memory: the rows are DuckDB's problem, not ours.
    pub fn add_view_from_scan(&mut self, sql_name: &str, path: &Path) -> Result<()> {
        let kind = crate::formats::large::ScanKind::for_path(path)
            .ok_or_else(|| anyhow!("{} cannot be scanned in place", path.display()))?;
        let name = sanitize_sql_name(sql_name);
        self.conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW {name} AS SELECT * FROM {}",
            kind.scan_expr(path)
        ))?;
        Ok(())
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
            .with_context(|| format!("dropping table {sql_name}"))?;
        Ok(())
    }

    /// Rename a registered workspace table, in DuckDB and in the registry, so
    /// the user picks what a `FROM` clause says. The name is sanitised the
    /// same way an import's is, so it never needs quoting.
    ///
    /// The active tab's table is refused here rather than only in the UI: the
    /// mutation path, the refresh button and the SQL job all address it by
    /// the name it was registered under.
    pub fn rename_table(&mut self, from: &str, to: &str) -> Result<()> {
        // Checked before sanitising: an empty name sanitises to "table", so
        // clearing the field and pressing Enter would silently rename to that.
        if to.trim().is_empty() {
            bail!("a table name cannot be empty");
        }
        let to = sanitize_sql_name(to);
        if from == to {
            return Ok(());
        }
        let Some(entry) = self.tables.get(from) else {
            bail!("no table named '{from}' in workspace");
        };
        if matches!(entry.origin, TableOrigin::ActiveTab) {
            bail!("'{from}' is the active tab's own table and keeps its name");
        }
        if self.tables.contains_key(&to) {
            bail!("the workspace already has a table named '{to}'");
        }
        self.conn
            .execute(
                &format!(
                    "ALTER TABLE {} RENAME TO {}",
                    quote_ident(from),
                    quote_ident(&to)
                ),
                [],
            )
            .with_context(|| format!("renaming {from} to {to}"))?;
        let mut entry = self.tables.remove(from).expect("present, checked above");
        entry.sql_name = to.clone();
        self.tables.insert(to, entry);
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
        // Catalogs and schemas too, so `alias.schema.table` completes at
        // every part. One statement, one round trip to an attached server.
        Self::push_first_column(
            &self.conn,
            "SELECT table_catalog FROM information_schema.tables \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog') \
             UNION SELECT table_schema FROM information_schema.tables \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog') \
             UNION SELECT table_name FROM information_schema.tables \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog')",
            &mut out,
        );
        Self::push_first_column(
            &self.conn,
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog')",
            &mut out,
        );
        // The paged-result tables and their ordering key live in the same
        // catalog, so information_schema hands them over too. They are
        // Octa's plumbing, not something to suggest to the user.
        out.retain(|n| !n.starts_with("__octa"));
        out.sort();
        out.dedup();
        out
    }

    /// Inspect a registered workspace table by its SQL name. Returns column
    /// list, row count, and up to `sample_rows` rows formatted as strings.
    /// Errors propagate as Err; the panel surfaces the message inline.
    pub fn inspect_registered_table(
        &self,
        sql_name: &str,
        sample_rows: usize,
    ) -> Result<TableInspection> {
        let Some(entry) = self.tables.get(sql_name) else {
            bail!("no registered workspace table '{sql_name}'");
        };
        // The count is known from the registration. Asking DuckDB again
        // would scan the whole file when `data` is a view over one.
        let rows = Some(entry.row_count);
        self.inspect_by_qualified_name(sql_name, sql_name, sample_rows, rows)
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
        self.inspect_by_qualified_name(&qualified, &display, sample_rows, None)
    }

    /// Column names + types of anything the workspace connection can address.
    /// A `DESCRIBE`, so it is metadata only - no `COUNT(*)`, no rows - which is
    /// what makes it safe to call for every table when building a prompt.
    fn columns_of(&self, qualified_sql: &str, display_name: &str) -> Result<Vec<ColumnInspection>> {
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
        Ok(columns)
    }

    /// Every table this workspace can query, with its columns, as
    /// `(name the user would type, columns)` - registered workspace tables
    /// first, then the tables inside each native attachment.
    ///
    /// For the Ask box: a question about an ATTACHed server can only be
    /// answered if the model is told what is in it, and inventing column names
    /// is the failure mode this exists to prevent. Metadata only (see
    /// [`Self::columns_of`]); the attachment listing comes from the same cache
    /// the panel already fills every frame. `max_tables` is a token budget, not
    /// a correctness limit, so tables past it are simply not described.
    pub fn schema_overview(&self, max_tables: usize) -> Vec<(String, Vec<ColumnInspection>)> {
        let mut out: Vec<(String, Vec<ColumnInspection>)> = Vec::new();
        for name in self.tables.keys() {
            if out.len() >= max_tables {
                return out;
            }
            if let Ok(cols) = self.columns_of(name, name) {
                out.push((name.clone(), cols));
            }
        }
        for (alias, attachment) in &self.attachments {
            if !attachment.native {
                continue; // its tables are already registered workspace entries
            }
            for t in self.list_attached_tables(alias).unwrap_or_default() {
                if out.len() >= max_tables {
                    return out;
                }
                let qualified = format!(
                    "{}.{}.{}",
                    quote_ident(alias),
                    quote_ident(&t.schema),
                    quote_ident(&t.table)
                );
                let display = format!("{alias}.{}.{}", t.schema, t.table);
                if let Ok(cols) = self.columns_of(&qualified, &display) {
                    out.push((display, cols));
                }
            }
        }
        out
    }

    /// `known_rows` skips the `COUNT(*)` when the caller already has it.
    fn inspect_by_qualified_name(
        &self,
        qualified_sql: &str,
        display_name: &str,
        sample_rows: usize,
        known_rows: Option<usize>,
    ) -> Result<TableInspection> {
        let columns = self.columns_of(qualified_sql, display_name)?;

        let count_sql = format!("SELECT COUNT(*) FROM {qualified_sql}");
        let row_count: Option<usize> = known_rows.or_else(|| {
            self.conn
                .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
                .ok()
                .map(|n| n as usize)
        });

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

    /// Run `query`, paging a plain SELECT; see [`Session::execute_paged`].
    /// The GUI runs it on the runner connection through [`Self::run_job`];
    /// this is the same call on the workspace's own connection.
    pub fn execute_paged(
        &mut self,
        query: &str,
        page_rows: usize,
        slot: u64,
    ) -> Result<PagedResult> {
        Session(&self.conn).execute_paged(query, page_rows, slot)
    }

    /// One page of the result [`Self::execute_paged`] last materialised.
    /// Errors when the last statement was not paged.
    pub fn result_page(&self, slot: u64, offset: usize, len: usize) -> Result<DataTable> {
        Session(&self.conn).result_page(slot, offset, len)
    }

    /// The whole materialised result, for Export and Write result to DB -
    /// neither may ship only the page that happens to be on screen.
    pub fn result_all(&self, slot: u64) -> Result<DataTable> {
        Session(&self.conn).result_all(slot)
    }

    /// Free a slot's materialised result (its editor was closed).
    pub fn drop_result(&self, slot: u64) {
        Session(&self.conn).drop_result(slot);
    }

    /// Execute a statement; see [`Session::execute`].
    pub fn execute(&mut self, query: &str) -> Result<QueryOutcome> {
        Session(&self.conn).execute(query)
    }

    /// The plan DuckDB would run for `query`, without running it: the JSON
    /// plan for the steps and the plain `EXPLAIN` text for the server's own
    /// view. Planning only, so a DELETE explained here deletes nothing.
    pub fn explain(&mut self, query: &str) -> Result<crate::sql::explain::Plan> {
        let q = crate::sql::explain::single_statement(query)?;
        let json = self.explain_value(&format!("EXPLAIN (FORMAT JSON) {q}"))?;
        let text = self.explain_value(&format!("EXPLAIN {q}"))?;
        Ok(crate::sql::explain::Plan {
            steps: crate::sql::explain::duckdb::parse(&json)?,
            server_text: crate::sql::explain::ascii_server_text(&text),
            bytes_scanned: None,
        })
    }

    /// The `explain_value` cell of an EXPLAIN statement's single row.
    fn explain_value(&mut self, sql: &str) -> Result<String> {
        let out = self.execute(sql)?;
        let col = out
            .table
            .columns
            .iter()
            .position(|c| c.name == "explain_value")
            .context("DuckDB returned no plan")?;
        out.table
            .rows
            .first()
            .and_then(|r| r.get(col))
            .map(|v| v.to_string())
            .context("DuckDB returned no plan")
    }

    // --- internal helpers ---

    fn register_or_replace(
        &mut self,
        sql_name: &str,
        table: &DataTable,
        origin: TableOrigin,
    ) -> Result<()> {
        // DROP first so re-registering `data` after an edit doesn't error.
        self.drop_relation(sql_name)?;
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

    fn drop_relation(&self, name: &str) -> Result<()> {
        Session(&self.conn).drop_relation(name)
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

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
