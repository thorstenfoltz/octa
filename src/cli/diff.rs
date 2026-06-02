//! `octa --diff FILE_A FILE_B` - row-level diff of two files.
//!
//! Reads both files through the format registry, delegates to the pure
//! `octa::data::diff::diff_rows`, and prints the rows unique to each side as a
//! single table (a leading `status` column tags each row `only_in_a` /
//! `only_in_b`). A one-line summary (`shared`, `only_in_a`, `only_in_b`
//! counts) goes to stderr so stdout stays a clean, parseable table.

use std::path::PathBuf;

use octa::data::diff::diff_rows;
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(path_a: PathBuf, path_b: PathBuf, format: OutputFormat) -> anyhow::Result<()> {
    let a = super::read_table(&path_a)?;
    let b = super::read_table(&path_b)?;
    let diff = diff_rows(&a, &b);

    let out = build_diff_table(&a, &b, &diff.only_in_a, &diff.only_in_b);
    write_table(&out, format)?;

    eprintln!(
        "shared {} row(s) - only in A: {} - only in B: {}",
        diff.shared_keys,
        diff.only_in_a.len(),
        diff.only_in_b.len()
    );
    Ok(())
}

/// One table tagging each differing row with its origin. Columns are
/// `status` + the canonical side's column names (A's, or B's when A has none).
fn build_diff_table(
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
