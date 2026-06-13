//! In-memory SQL execution against `DataTable`s via DuckDB.
//!
//! Two surfaces live here:
//!
//! - [`run_query`] is the one-shot legacy entry point: open a fresh
//!   in-memory DuckDB connection, register the caller's `DataTable` as the
//!   temp table `data`, execute one statement, tear the connection down.
//!   Every existing caller (the GUI's `Run` button, the CLI `--sql` action,
//!   the MCP `run_sql` single-file mode) keeps calling this and behaves
//!   exactly as it did before the workspace was introduced.
//!
//! - [`SqlWorkspace`] is the persistent multi-table surface: a single
//!   connection holds the user's `data` table plus any number of
//!   additional tables (loaded from any supported format via
//!   `FormatRegistry`) and zero or more ATTACH-ed DuckDB/SQLite databases.
//!   JOINs across heterogeneous sources, schema-qualified queries, and
//!   write-back to a real DB file all live on the workspace.
//!
//! Internally `run_query` is a one-line wrapper that builds a workspace,
//! registers `data`, and calls `execute`. There is no separate execution
//! path: everything goes through [`SqlWorkspace`].

mod engine;
mod workspace;

use anyhow::Result;

use crate::data::DataTable;

pub use engine::{QueryKind, QueryOutcome};
pub use workspace::{
    AttachKind, AttachedTable, Attachment, ColumnInspection, RegisteredTable, SqlWorkspace,
    TableInspection, TableOrigin, WriteMode, WriteReport, WriteTarget, dedupe_sql_name,
    sanitize_sql_name,
};

/// Execute `query` against `table`, returning a classified outcome.
/// The table is exposed in SQL as `data`. Identifiers in the schema are
/// quoted, so column names with spaces or punctuation are preserved.
///
/// On mutations the returned table is re-stamped with the source table's
/// schema, `source_path`, `format_name`, and (for DB-backed sources) a
/// fresh `db_meta` snapshot so the GUI's mutation flow can replace the
/// active table and have a follow-up Save still know which DB row identity
/// to diff against.
pub fn run_query(table: &DataTable, query: &str) -> Result<QueryOutcome> {
    let mut ws = SqlWorkspace::new()?;
    ws.set_active_table(table)?;
    let mut outcome = ws.execute(query)?;
    if outcome.kind == QueryKind::Mutation {
        if outcome.table.columns.len() == table.columns.len() {
            outcome.table.columns = table.columns.clone();
        }
        outcome.table.source_path = table.source_path.clone();
        outcome.table.format_name = table.format_name.clone();
        outcome.table.structural_changes = true;
        if let Some(meta) = table.db_meta.as_ref() {
            let row_count = outcome.table.row_count();
            outcome.table.db_meta = Some(crate::data::DbRowMeta {
                table_name: meta.table_name.clone(),
                schema: meta.schema.clone(),
                row_tags: vec![None; row_count],
                original: meta.original.clone(),
                original_columns: meta.original_columns.clone(),
            });
        }
    }
    Ok(outcome)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
