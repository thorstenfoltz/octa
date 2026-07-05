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
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(cols: &[&str], rows: &[&[&str]]) -> DataTable {
        let columns = cols
            .iter()
            .map(|c| ColumnInfo {
                name: c.to_string(),
                data_type: "Utf8".to_string(),
            })
            .collect();
        let rows = rows
            .iter()
            .map(|r| r.iter().map(|v| CellValue::String(v.to_string())).collect())
            .collect();
        DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: None,
            format_name: None,
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    #[test]
    fn transposes_shape_and_values() {
        // 2 columns x 3 rows -> 3 (+1 header) columns x 2 rows.
        let t = table(
            &["id", "name"],
            &[&["1", "ann"], &["2", "bob"], &["3", "cy"]],
        );
        let out = transpose_table(&t);
        // Columns: "column", "1", "2", "3".
        assert_eq!(
            out.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["column", "1", "2", "3"]
        );
        // Two output rows, one per original column.
        assert_eq!(out.rows.len(), 2);
        assert_eq!(out.rows[0][0], CellValue::String("id".into()));
        assert_eq!(out.rows[0][1], CellValue::String("1".into()));
        assert_eq!(out.rows[0][3], CellValue::String("3".into()));
        assert_eq!(out.rows[1][0], CellValue::String("name".into()));
        assert_eq!(out.rows[1][2], CellValue::String("bob".into()));
        // All output columns are text.
        assert!(out.columns.iter().all(|c| c.data_type == "Utf8"));
    }

    #[test]
    fn empty_table_gives_no_rows() {
        let t = table(&[], &[]);
        let out = transpose_table(&t);
        assert_eq!(out.columns.len(), 1); // just the leading "column".
        assert!(out.rows.is_empty());
    }

    #[test]
    fn ragged_missing_cells_become_empty() {
        // Second row is short; missing cell reads as empty text.
        let mut t = table(&["a", "b"], &[&["1", "2"]]);
        t.rows.push(vec![CellValue::String("3".into())]); // missing col b.
        let out = transpose_table(&t);
        // Row for column "b": header + r0="2" + r1="" (missing).
        assert_eq!(out.rows[1][0], CellValue::String("b".into()));
        assert_eq!(out.rows[1][2], CellValue::String(String::new()));
    }
}
