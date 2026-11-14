use super::*;
use crate::data::{CellValue, ColumnInfo, TextOrder};

/// Five rows holding 0..5 in one column, so a row can be told by its value.
fn numbered() -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        }],
        rows: (0..5).map(|n| vec![CellValue::Int(n)]).collect(),
        ..DataTable::empty()
    }
}

/// The values of the hidden rows, sorted: which rows are hidden, wherever
/// they sit now.
fn hidden_values(table: &DataTable) -> Vec<i64> {
    let mut values: Vec<i64> = table
        .hidden_rows
        .iter()
        .map(|&r| match table.get(r, 0) {
            Some(CellValue::Int(n)) => *n,
            other => panic!("row {r} holds {other:?}"),
        })
        .collect();
    values.sort_unstable();
    values
}

#[test]
fn hiding_ignores_rows_that_do_not_exist() {
    let mut table = numbered();
    table.hide_rows([1, 3, 99]);
    assert_eq!(hidden_values(&table), [1, 3]);
    table.show_hidden_rows();
    assert!(table.hidden_rows.is_empty());
}

#[test]
fn a_hidden_row_stays_hidden_when_rows_are_inserted_and_deleted_above_it() {
    let mut table = numbered();
    table.hide_rows([1, 3]);

    table.insert_row(0);
    table.set(0, 0, CellValue::Int(100));
    assert_eq!(hidden_values(&table), [1, 3]);
    table.undo();
    table.undo();
    assert_eq!(hidden_values(&table), [1, 3]);
    table.redo();
    assert_eq!(hidden_values(&table), [1, 3]);
    table.undo();

    table.delete_row(0);
    assert_eq!(hidden_values(&table), [1, 3]);
    table.undo();
    assert_eq!(hidden_values(&table), [1, 3]);
    table.redo();
    assert_eq!(hidden_values(&table), [1, 3]);
}

#[test]
fn deleting_a_hidden_row_takes_it_out_of_the_set() {
    let mut table = numbered();
    table.hide_rows([1, 3]);
    table.delete_row(1);
    assert_eq!(hidden_values(&table), [3]);
    assert_eq!(table.hidden_rows.len(), 1);
}

#[test]
fn a_hidden_row_stays_hidden_through_a_sort_and_its_undo() {
    let mut table = numbered();
    table.hide_rows([0, 1]);
    table.sort_rows_by_columns(&[(0, false)], TextOrder::default());
    assert_eq!(
        table.get(0, 0),
        Some(&CellValue::Int(4)),
        "sorted descending"
    );
    assert_eq!(hidden_values(&table), [0, 1]);
    table.undo();
    assert_eq!(hidden_values(&table), [0, 1]);
}

#[test]
fn rows_dropped_from_the_front_take_their_place_in_the_set_along() {
    let mut table = numbered();
    table.hide_rows([0, 3]);
    table.evict_front_rows(2);
    assert_eq!(hidden_values(&table), [3]);
}
