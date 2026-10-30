//! Transpose a table: rows become columns and columns become rows.
//!
//! Pure and self-contained so it can be unit-tested and reused. The GUI snapshots
//! the active table (applying pending edits) before calling this, then opens the
//! result in a detached tab.

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Maximum source rows the GUI will transpose. Each original row becomes an
/// output column, so a large table would produce an unusable number of columns.
pub const TRANSPOSE_MAX_ROWS: usize = 1000;

/// Transpose `table`. The output has a leading `column` column holding the
/// original column names, then one column per original row (named `1`..`N`).
/// Every output cell is text (`Utf8`), since a transposed mix of column types
/// has no single type.
pub fn transpose_table(table: &DataTable) -> DataTable {
    let n_rows = table.rows.len();
    let n_cols = table.columns.len();

    let mut columns: Vec<ColumnInfo> = Vec::with_capacity(n_rows + 1);
    columns.push(ColumnInfo {
        name: "column".to_string(),
        data_type: "Utf8".to_string(),
    });
    for r in 0..n_rows {
        columns.push(ColumnInfo {
            name: (r + 1).to_string(),
            data_type: "Utf8".to_string(),
        });
    }

    // One output row per original column: [original header, cell(r=0), cell(r=1), ...].
    let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(n_cols);
    for (c, col) in table.columns.iter().enumerate() {
        let mut out_row: Vec<CellValue> = Vec::with_capacity(n_rows + 1);
        out_row.push(CellValue::String(col.name.clone()));
        for row in table.rows.iter() {
            let text = row.get(c).map(|v| v.to_string()).unwrap_or_default();
            out_row.push(CellValue::String(text));
        }
        rows.push(out_row);
    }

    DataTable {
        columns,
        rows,
        ..DataTable::empty()
    }
}

#[cfg(test)]
#[path = "transpose_tests.rs"]
mod tests;
