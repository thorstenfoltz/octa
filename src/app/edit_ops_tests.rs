use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable, SearchMode};

/// Ten rows numbered 0..10 in one column, nothing selected.
fn tab() -> TabState {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = DataTable {
        columns: vec![ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        }],
        rows: (0..10).map(|n| vec![CellValue::Int(n)]).collect(),
        ..DataTable::empty()
    };
    tab
}

fn values(tab: &TabState) -> Vec<i64> {
    (0..tab.table.row_count())
        .map(|r| match tab.table.get(r, 0) {
            Some(CellValue::Int(n)) => *n,
            other => panic!("row {r}: {other:?}"),
        })
        .collect()
}

#[test]
fn deleting_marked_rows_deletes_every_one_and_one_undo_restores_them() {
    let mut tab = tab();
    tab.table_state.selected_rows = [2, 5, 7].into();
    tab.table_state.selected_cell = Some((7, 0));
    // A right-click on one of the marked rows, the user's report.
    tab.delete_rows_from(Some(7));
    assert_eq!(values(&tab), [0, 1, 3, 4, 6, 8, 9]);
    assert_eq!(tab.table_state.selected_cell, Some((2, 0)));
    assert!(tab.table_state.selected_rows.is_empty());

    assert!(tab.table.undo());
    assert_eq!(values(&tab), (0..10).collect::<Vec<_>>());
}

#[test]
fn the_menu_and_shortcut_delete_the_whole_selection() {
    let mut tab = tab();
    tab.table_state.selected_rows = [0, 9].into();
    tab.delete_rows_from(None);
    assert_eq!(values(&tab), [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_right_click_outside_the_selection_deletes_only_that_row() {
    let mut tab = tab();
    tab.table_state.selected_rows = [2, 5].into();
    tab.delete_rows_from(Some(8));
    assert_eq!(values(&tab), [0, 1, 2, 3, 4, 5, 6, 7, 9]);
}

#[test]
fn without_marked_rows_the_cursor_row_goes() {
    let mut tab = tab();
    tab.table_state.selected_cell = Some((9, 0));
    tab.delete_rows_from(None);
    assert_eq!(values(&tab), (0..9).collect::<Vec<_>>());
    assert_eq!(tab.table_state.selected_cell, Some((8, 0)));
}
