use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable, SearchMode};

/// Ten rows, three columns, nothing selected.
fn tab() -> TabState {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = DataTable {
        columns: ["a", "b", "c"]
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).to_string(),
                data_type: "Int64".into(),
            })
            .collect(),
        rows: (0..10).map(|n| vec![CellValue::Int(n); 3]).collect(),
        ..DataTable::empty()
    };
    tab.filtered_rows = (0..10).collect();
    tab
}

#[test]
fn hidden_rows_leave_the_view_and_come_back() {
    let mut tab = tab();
    tab.table_state.selected_rows = [2, 5, 7].into();
    assert_eq!(tab.hide_rows_block(), None);
    tab.hide_rows_from(None);
    assert!(tab.filter_dirty, "the view has to be filtered again");
    tab.drop_hidden_rows();
    assert_eq!(tab.filtered_rows, [0, 1, 3, 4, 6, 8, 9]);
    assert_eq!(tab.table.row_count(), 10, "the rows stay in the table");
    assert!(tab.table_state.selected_rows.is_empty());

    tab.show_hidden_rows();
    assert!(tab.table.hidden_rows.is_empty());
}

#[test]
fn a_right_click_hides_the_selection_only_when_the_row_is_part_of_it() {
    let mut tab = tab();
    tab.table_state.selected_rows = [2, 5].into();
    tab.hide_rows_from(Some(8));
    assert_eq!(tab.table.hidden_rows, [8].into());

    tab.table_state.selected_rows = [2, 5].into();
    tab.hide_rows_from(Some(5));
    assert_eq!(tab.table.hidden_rows, [2, 5, 8].into());
}

#[test]
fn without_selected_rows_the_selected_cells_name_the_rows_and_columns() {
    let mut tab = tab();
    assert!(tab.hide_rows_block().is_some(), "nothing is selected");
    assert!(tab.hide_columns_block().is_some(), "nothing is selected");

    tab.table_state.selected_cell = Some((4, 1));
    tab.table_state.selected_cells = [(4, 1), (6, 2)].into();
    assert_eq!(tab.hide_row_targets(), [4, 6].into());
    assert_eq!(tab.hide_column_targets(), [1, 2].into());
}

#[test]
fn the_last_visible_column_cannot_be_hidden() {
    let mut tab = tab();
    tab.table_state.selected_cols = [0, 1].into();
    assert_eq!(tab.hide_columns_block(), None);
    tab.hide_columns_from(None);
    assert_eq!(tab.hidden_columns, [0, 1].into());

    tab.table_state.selected_cols = [2].into();
    assert!(tab.hide_columns_block().is_some());
    tab.hide_columns_from(None);
    tab.hide_columns_from(Some(2));
    assert_eq!(tab.hidden_columns, [0, 1].into(), "column c stays");
}

#[test]
fn a_right_click_on_a_column_outside_the_selection_hides_only_that_column() {
    let mut tab = tab();
    tab.table_state.selected_cols = [0, 1].into();
    tab.hide_columns_from(Some(2));
    assert_eq!(tab.hidden_columns, [2].into());
}
