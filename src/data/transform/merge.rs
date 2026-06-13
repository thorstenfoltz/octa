//! Merge several columns into one.

use super::cell_text;
use crate::data::{CellValue, DataTable};

/// Join the text of `cols` (in the given order) with `separator`, one new
/// `String` cell per row. Out-of-range column indices contribute an empty
/// string.
pub fn merge_columns(table: &DataTable, cols: &[usize], separator: &str) -> Vec<CellValue> {
    let n = table.row_count();
    (0..n)
        .map(|r| {
            let joined = cols
                .iter()
                .map(|&c| cell_text(table, r, c))
                .collect::<Vec<_>>()
                .join(separator);
            CellValue::String(joined)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{ColumnInfo, DataTable};

    fn two_col(rows: &[(&str, &str)]) -> DataTable {
        let mut t = DataTable::empty();
        for name in ["a", "b"] {
            t.columns.push(ColumnInfo {
                name: name.to_string(),
                data_type: "Utf8".to_string(),
            });
        }
        t.rows = rows
            .iter()
            .map(|(a, b)| {
                vec![
                    CellValue::String(a.to_string()),
                    CellValue::String(b.to_string()),
                ]
            })
            .collect();
        t
    }

    #[test]
    fn merge_with_separator() {
        let t = two_col(&[("John", "Doe"), ("Jane", "Roe")]);
        let out = merge_columns(&t, &[0, 1], " ");
        assert_eq!(out[0], CellValue::String("John Doe".into()));
        assert_eq!(out[1], CellValue::String("Jane Roe".into()));
    }

    #[test]
    fn null_becomes_empty() {
        let mut t = two_col(&[("x", "y")]);
        t.rows[0][1] = CellValue::Null;
        let out = merge_columns(&t, &[0, 1], "-");
        assert_eq!(out[0], CellValue::String("x-".into()));
    }
}
