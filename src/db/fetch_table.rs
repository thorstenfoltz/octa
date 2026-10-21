//! Read one database table into a `DataTable`, so the same comparison engine
//! that diffs two files can diff a file against a live table.
//!
//! No new comparison logic lives here: this is only the adapter that turns a
//! connection plus a table name into the shape `octa::data::compare` already
//! takes. The read is capped by `initial_load_rows` like every other read, and
//! the caller is expected to surface that cap.

use crate::data::DataTable;
use crate::db::{DbConnection, connect, select_sample_sql};

/// Split `catalog.schema.table`, `schema.table` or `table` into its parts.
///
/// An unqualified name yields an empty schema rather than a guess; the caller
/// fills it in with the connection's own default, which is the only place that
/// knows what that is.
pub fn split_qualified(spec: &str) -> (Option<String>, String, String) {
    let parts: Vec<&str> = spec.split('.').collect();
    match parts.as_slice() {
        [table] => (None, String::new(), (*table).to_string()),
        [schema, table] => (None, (*schema).to_string(), (*table).to_string()),
        [catalog, schema, table] => (
            Some((*catalog).to_string()),
            (*schema).to_string(),
            (*table).to_string(),
        ),
        // More than three parts: treat everything before the last two as the
        // catalog, so a dotted catalog name still resolves.
        _ => (
            Some(parts[..parts.len() - 2].join(".")),
            parts[parts.len() - 2].to_string(),
            parts[parts.len() - 1].to_string(),
        ),
    }
}

/// Fetch a table's rows through a fresh connection.
///
/// `schema` may be empty, in which case the connection's default database name
/// is used, matching what the sidebar does when it opens a table.
pub fn fetch_table(
    conn: &DbConnection,
    secret: Option<&str>,
    ssh_secret: Option<&str>,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> anyhow::Result<DataTable> {
    let schema = if !schema.is_empty() {
        schema
    } else if conn.engine.is_file() {
        // A file's Database field is its path, not a schema.
        "main"
    } else {
        conn.database.as_str()
    };
    let mut connector = connect(conn, secret, ssh_secret)?;
    // The streaming cap, deliberately, not the GUI's `db_page_rows`: the
    // sidebar loads a page because the user can scroll for the next one, and
    // CLI and MCP cannot. A headless read gets the whole cap in one go.
    let sql = select_sample_sql(
        conn.engine,
        catalog,
        schema,
        table,
        crate::formats::initial_load_rows(),
    );
    connector.query(&sql)
}

#[cfg(test)]
#[path = "fetch_table_tests.rs"]
mod tests;
