//! Fill empty cells from the previous (down) or next (up) non-empty cell.

use super::is_empty;
use crate::data::{CellValue, DataTable};

/// Return `col`'s values with each empty cell replaced by the most recent
/// non-empty value above it (classic spreadsheet "fill down"). Leading empties
/// stay `Null`.
pub fn fill_down(table: &DataTable, col: usize) -> Vec<CellValue> {
    let n = table.row_count();
    let mut out = Vec::with_capacity(n);
    let mut last: Option<CellValue> = None;
    for r in 0..n {
        let cur = table.get(r, col).cloned().unwrap_or(CellValue::Null);
        if is_empty(&cur) {
            out.push(last.clone().unwrap_or(CellValue::Null));
        } else {
            last = Some(cur.clone());
            out.push(cur);
        }
    }
    out
}

/// Mirror of [`fill_down`] working upward: empties take the next non-empty
/// value below them. Trailing empties stay `Null`.
pub fn fill_up(table: &DataTable, col: usize) -> Vec<CellValue> {
    let n = table.row_count();
    let mut out = vec![CellValue::Null; n];
    let mut next: Option<CellValue> = None;
    for r in (0..n).rev() {
        let cur = table.get(r, col).cloned().unwrap_or(CellValue::Null);
        if is_empty(&cur) {
            out[r] = next.clone().unwrap_or(CellValue::Null);
        } else {
            next = Some(cur.clone());
            out[r] = cur;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{ColumnInfo, DataTable};

    fn col(values: &[Option<&str>]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns.push(ColumnInfo {
            name: "c".to_string(),
            data_type: "Utf8".to_string(),
        });
        t.rows = values
            .iter()
            .map(|v| {
                vec![match v {
                    Some(s) => CellValue::String(s.to_string()),
                    None => CellValue::Null,
                }]
            })
            .collect();
        t
    }

    #[test]
    fn down_propagates_and_keeps_leading_null() {
        let t = col(&[None, Some("A"), None, None, Some("B"), None]);
        let out = fill_down(&t, 0);
        assert_eq!(out[0], CellValue::Null);
        assert_eq!(out[2], CellValue::String("A".into()));
        assert_eq!(out[3], CellValue::String("A".into()));
        assert_eq!(out[5], CellValue::String("B".into()));
    }

    #[test]
    fn up_propagates_and_keeps_trailing_null() {
        let t = col(&[None, Some("A"), None, Some("B"), None]);
        let out = fill_up(&t, 0);
        assert_eq!(out[0], CellValue::String("A".into()));
        assert_eq!(out[2], CellValue::String("B".into()));
        assert_eq!(out[4], CellValue::Null);
    }
}
