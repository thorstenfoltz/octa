//! `octa --sync-sql FILE --db NAME --sync-table SCHEMA.TABLE --sync-on COLS`
//!
//! Prints the SQL that would make the live table match FILE, and sends
//! nothing to the server beyond the SELECT that reads it. The rendering is
//! `db::write_back::render_plan_sql`, the same function the GUI's Save SQL
//! writes, so a reviewed script and an applied write-back cannot drift.
//!
//! Direction: the file is the desired state, the table is the current one.
//! Rows only on the server become DELETEs, rows only in the file become
//! INSERTs, and matched rows whose cells differ become full-row UPDATEs
//! addressed by the key columns.

use std::path::PathBuf;

use anyhow::{Result, bail};

use octa::data::compare::compare_join;
use octa::db::write_back::{plan_from_comparison, render_plan_sql};

pub fn run(path: PathBuf, db: String, table_spec: String, on: String) -> Result<()> {
    let keys: Vec<String> = on
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if keys.is_empty() {
        bail!("--sync-on needs at least one column name");
    }

    let file_table = super::read_table(&path)?;
    for key in &keys {
        if !file_table.columns.iter().any(|c| &c.name == key) {
            bail!(
                "--sync-on column '{key}' is not in {}; available: {}",
                path.display(),
                file_table
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    let (conn, settings) = super::db::find_connection_pub(&db)?;
    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
    let (catalog, schema, table) = octa::db::fetch_table::split_qualified(&table_spec);
    let live = octa::db::fetch_table::fetch_table(
        &conn,
        secret.as_deref(),
        catalog.as_deref(),
        &schema,
        &table,
    )?;

    let result = compare_join(&live, &file_table, &keys)?;
    let synced = plan_from_comparison(&live, &file_table, &result, &keys)?;
    let plan = synced.plan;
    if !synced.ignored_columns.is_empty() {
        eprintln!(
            "note: {} column(s) exist in the file but not in the table and are ignored: {}",
            synced.ignored_columns.len(),
            synced.ignored_columns.join(", ")
        );
    }

    print!(
        "{}",
        render_plan_sql(
            conn.engine,
            &schema,
            &table,
            &file_table.columns,
            &keys,
            &plan,
        )
    );
    eprintln!(
        "{} delete(s), {} update(s), {} insert(s)",
        plan.deletes.len(),
        plan.updates.len(),
        plan.inserts.len()
    );
    Ok(())
}
