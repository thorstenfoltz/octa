//! `octa --diff FILE_A FILE_B [--diff-mode MODE] [--diff-on COLS]` - data
//! comparison of two files.
//!
//! Three modes (see [`octa::data::compare::CompareMode`]):
//! * `set` (default) - whole-row membership via `octa::data::diff::diff_rows`;
//!   prints rows unique to each side tagged `only_in_a` / `only_in_b`.
//! * `ordered` - positional row-by-row comparison; prints rows unique to the
//!   longer side plus paired `changed_a` / `changed_b` rows for matched rows
//!   whose cells differ.
//! * `join` - matches rows on the `--diff-on` key column(s) and prints
//!   added / removed / changed rows.
//!
//! A one-line summary goes to stderr so stdout stays a clean, parseable table.

use std::path::PathBuf;

use octa::data::compare::{self, CompareMode};
use octa::data::diff::diff_rows;
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    path_a: PathBuf,
    path_b: Option<PathBuf>,
    db_b: Option<(String, String)>,
    mode: CompareMode,
    on: Vec<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let a = super::read_table(&path_a)?;
    let b = match (&path_b, &db_b) {
        (Some(p), _) => super::read_table(p)?,
        (None, Some((conn_name, spec))) => read_db_side(conn_name, spec)?,
        (None, None) => {
            anyhow::bail!("--diff needs a second file, or --diff-db with --diff-db-table")
        }
    };

    match mode {
        CompareMode::Set => {
            let diff = diff_rows(&a, &b);
            let out = build_set_table(&a, &b, &diff.only_in_a, &diff.only_in_b);
            write_table(&out, format)?;
            eprintln!(
                "mode set - shared {} row(s) - only in A: {} - only in B: {}",
                diff.shared_keys,
                diff.only_in_a.len(),
                diff.only_in_b.len()
            );
        }
        CompareMode::Ordered | CompareMode::Join => {
            let result = match mode {
                CompareMode::Ordered => compare::compare_ordered(&a, &b),
                CompareMode::Join => compare::compare_join(&a, &b, &on)?,
                CompareMode::Set => unreachable!(),
            };
            let out = compare::build_compare_table(&a, &b, &result);
            write_table(&out, format)?;
            eprintln!(
                "mode {} - unchanged: {} - changed: {} - only in A: {} - only in B: {}",
                mode.as_str(),
                result.unchanged,
                result.changed.len(),
                result.only_in_a.len(),
                result.only_in_b.len()
            );
        }
    }
    Ok(())
}

/// One table tagging each differing row with its origin (set mode). Columns are
/// `status` + the canonical side's column names (A's, or B's when A has none).
fn build_set_table(
    a: &DataTable,
    b: &DataTable,
    only_in_a: &[usize],
    only_in_b: &[usize],
) -> DataTable {
    let canonical = if a.col_count() > 0 { a } else { b };
    let ncols = canonical.col_count();

    let mut columns = Vec::with_capacity(ncols + 1);
    columns.push(ColumnInfo {
        name: "status".to_string(),
        data_type: "Utf8".to_string(),
    });
    columns.extend(canonical.columns.iter().cloned());

    let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(only_in_a.len() + only_in_b.len());
    let mut push_side = |table: &DataTable, indices: &[usize], status: &str| {
        for &r in indices {
            let mut row = Vec::with_capacity(ncols + 1);
            row.push(CellValue::String(status.to_string()));
            for c in 0..ncols {
                row.push(table.get(r, c).cloned().unwrap_or(CellValue::Null));
            }
            rows.push(row);
        }
    };
    push_side(a, only_in_a, "only_in_a");
    push_side(b, only_in_b, "only_in_b");

    let mut out = DataTable::empty();
    out.columns = columns;
    out.rows = rows;
    out
}

/// Read the B side from a saved database connection. Same cap as any other
/// read, so a large table is compared on its first `initial_load_rows` rows;
/// the caller's summary line says so.
fn read_db_side(conn_name: &str, spec: &str) -> anyhow::Result<octa::data::DataTable> {
    let settings = octa::ui::settings::AppSettings::load();
    let conn = settings
        .db_connections
        .iter()
        .find(|c| c.name == conn_name)
        .ok_or_else(|| anyhow::anyhow!("no saved database connection named '{conn_name}'"))?;
    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
    let ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
    let (catalog, schema, table) = octa::db::fetch_table::split_qualified(spec);
    octa::db::fetch_table::fetch_table(
        conn,
        secret.as_deref(),
        ssh_secret.as_deref(),
        catalog.as_deref(),
        &schema,
        &table,
    )
}
