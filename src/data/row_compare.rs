//! Compare N rows of one table, field by field.
//!
//! The output is the transpose of just those rows - one output row per
//! original column - with a `differs` flag in front, so the eye runs down a
//! single narrow column to find where the records disagree:
//!
//! ```text
//! column   differs  row 12   row 4711  row 900001
//! city     no       Aachen   Aachen    Aachen
//! amount   yes      12.50    13.10     12.50
//! ```
//!
//! Built for arbitrary N, never for two: three marked rows are three columns.
//! That is why this does not go through [`crate::data::compare`], whose A-vs-B
//! `status` / `changed_columns` shape only ever describes a pair.
//!
//! Pure, and composed of two engines that already exist:
//! [`crate::data::compare::subset`] pulls the rows out and
//! [`crate::data::transpose::transpose_table`] turns them on their side.

use std::collections::HashSet;

use crate::data::compare::subset;
use crate::data::transpose::transpose_table;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// Column holding the `yes` flag, in front of the compared rows.
const DIFFERS_COL: usize = 1;

/// The rows to compare: the selection when it holds at least two rows, else
/// the marked rows.
///
/// Selecting is the lighter gesture and therefore wins, which is what makes
/// the entry usable without marking anything first. It only wins when it holds
/// two or more rows, so a single click that lands on a row does not silently
/// replace a set of marks the user built up on purpose. One place, because the
/// menu asks how many rows there are and the tab opener asks which ones.
pub fn rows_to_compare(table: &DataTable, selected: &HashSet<usize>) -> Vec<usize> {
    if selected.len() >= 2 {
        let mut rows: Vec<usize> = selected.iter().copied().collect();
        rows.sort_unstable();
        return rows;
    }
    marked_rows(table)
}

/// The rows the user has marked, ascending.
///
/// Only whole-row marks count: a marked *cell* says something about that cell,
/// and reading it as "compare this row" would put rows in the comparison the
/// user never asked for. One place, because the menu asks whether there are
/// enough of them and the tab opener asks which ones.
pub fn marked_rows(table: &DataTable) -> Vec<usize> {
    let mut rows: Vec<usize> = table
        .marks
        .keys()
        .filter_map(|k| match k {
            crate::data::MarkKey::Row(r) => Some(*r),
            _ => None,
        })
        .collect();
    rows.sort_unstable();
    rows
}

/// Compare `rows` of `table` field by field.
///
/// Row labels carry `row_offset`, so a page of a large file names the rows the
/// user sees. Values are compared as the text the transpose produced, which is
/// exactly what the resulting tab displays: a cell cannot look identical and
/// still be flagged, or the flag would be untrustworthy.
pub fn compare_rows(table: &DataTable, rows: &[usize]) -> DataTable {
    let mut out = transpose_table(&subset(table, rows));

    for (i, &r) in rows.iter().enumerate() {
        if let Some(col) = out.columns.get_mut(i + 1) {
            col.name = format!("row {}", r + 1 + table.row_offset);
        }
    }

    out.columns.insert(
        DIFFERS_COL,
        ColumnInfo {
            name: "differs".to_string(),
            data_type: "Utf8".to_string(),
        },
    );
    for row in &mut out.rows {
        let differs = row[DIFFERS_COL..]
            .iter()
            .any(|v| v.to_string() != row[DIFFERS_COL].to_string());
        // Spelled out both ways: an empty cell reads as "not checked" and
        // leaves the reader guessing which it is.
        let flag = if differs { "yes" } else { "no" };
        row.insert(DIFFERS_COL, CellValue::String(flag.to_string()));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = ["city", "amount"]
            .iter()
            .map(|n| ColumnInfo {
                name: n.to_string(),
                data_type: "Utf8".to_string(),
            })
            .collect();
        t.rows = vec![
            vec![
                CellValue::String("Aachen".into()),
                CellValue::Float(12.5_f64),
            ],
            vec![
                CellValue::String("Aachen".into()),
                CellValue::Float(13.1_f64),
            ],
            vec![CellValue::String("Bonn".into()), CellValue::Float(12.5_f64)],
        ];
        t
    }

    #[test]
    fn flags_only_the_fields_that_differ() {
        let out = compare_rows(&table(), &[0, 1]);
        assert_eq!(
            out.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["column", "differs", "row 1", "row 2"]
        );
        // city is the same in both rows, amount is not.
        assert_eq!(out.get(0, 1).unwrap().to_string(), "no");
        assert_eq!(out.get(1, 1).unwrap().to_string(), "yes");
    }

    #[test]
    fn three_rows_are_three_columns() {
        let out = compare_rows(&table(), &[0, 1, 2]);
        assert_eq!(out.col_count(), 5);
        // A field two rows agree on still differs once the third disagrees.
        assert_eq!(out.get(0, 1).unwrap().to_string(), "yes");
        assert_eq!(out.get(0, 2).unwrap().to_string(), "Aachen");
        assert_eq!(out.get(0, 4).unwrap().to_string(), "Bonn");
    }

    #[test]
    fn one_row_differs_from_nothing() {
        let out = compare_rows(&table(), &[2]);
        assert_eq!(out.col_count(), 3);
        assert!(out.rows.iter().all(|r| r[1].to_string() == "no"));
    }

    /// A selection of two or more rows is enough on its own, and it wins over
    /// whatever was marked earlier.
    #[test]
    fn a_selection_beats_the_marks_but_one_selected_row_does_not() {
        use crate::data::{MarkColor, MarkKey};
        let mut t = table();
        t.set_mark(MarkKey::Row(0), MarkColor::Red);
        t.set_mark(MarkKey::Row(1), MarkColor::Red);
        assert_eq!(rows_to_compare(&t, &HashSet::new()), vec![0, 1]);
        assert_eq!(rows_to_compare(&t, &HashSet::from([2])), vec![0, 1]);
        assert_eq!(rows_to_compare(&t, &HashSet::from([2, 1])), vec![1, 2]);
    }

    #[test]
    fn only_whole_row_marks_count_and_they_come_out_sorted() {
        use crate::data::{MarkColor, MarkKey};
        let mut t = table();
        t.set_mark(MarkKey::Row(2), MarkColor::Red);
        t.set_mark(MarkKey::Row(0), MarkColor::Red);
        t.set_mark(MarkKey::Cell(1, 0), MarkColor::Blue);
        t.set_mark(MarkKey::Column(1), MarkColor::Green);
        assert_eq!(marked_rows(&t), vec![0, 2]);
    }

    #[test]
    fn row_labels_follow_the_page_offset() {
        let mut t = table();
        t.row_offset = 900_000;
        let out = compare_rows(&t, &[0, 2]);
        assert_eq!(out.columns[2].name, "row 900001");
        assert_eq!(out.columns[3].name, "row 900003");
    }
}
