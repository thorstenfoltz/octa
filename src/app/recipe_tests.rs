use octa::data::recipe::{RecipeStep, SetCells};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::TabState;

fn tab(headers: &[&str], rows: &[&[&str]]) -> TabState {
    let mut t = TabState::new(octa::data::SearchMode::Plain);
    let mut d = DataTable::empty();
    d.columns = headers
        .iter()
        .map(|h| ColumnInfo {
            name: h.to_string(),
            data_type: "Utf8".into(),
        })
        .collect();
    d.rows = rows
        .iter()
        .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
        .collect();
    t.table = d;
    t
}

fn set_cells(t: &TabState, i: usize) -> SetCells {
    match &t.recipe[i].step {
        RecipeStep::SetCells(s) => s.clone(),
        other => panic!("expected set_cells, got {other:?}"),
    }
}

#[test]
fn a_typed_value_is_recorded_against_the_rows_id() {
    let mut t = tab(&["name", "id"], &[&["a", "7"], &["b", "8"]]);
    t.table.set(1, 0, CellValue::String("B".into()));
    assert!(t.record_hand_edits());
    let s = set_cells(&t, 0);
    assert_eq!(s.key, ["id"]);
    assert_eq!(s.cells[0].row, ["8"]);
    assert_eq!(s.cells[0].column, "name");
    assert_eq!(s.cells[0].value.as_deref(), Some("B"));
    // Nothing new: nothing recorded twice.
    assert!(!t.record_hand_edits());
}

#[test]
fn editing_the_id_itself_keeps_the_old_id_to_find_the_row() {
    let mut t = tab(&["id", "v"], &[&["7", "x"]]);
    t.table.set(0, 0, CellValue::String("70".into()));
    t.record_hand_edits();
    assert_eq!(set_cells(&t, 0).cells[0].row, ["7"]);
}

#[test]
fn without_an_id_the_edit_waits_and_resolves_when_one_is_chosen() {
    let mut t = tab(&["city", "price"], &[&["Oslo", "1"], &["Oslo", "2"]]);
    t.table.set(1, 1, CellValue::String("9".into()));
    t.record_hand_edits();
    assert!(t.recipe[0].pending.is_some());
    assert!(
        t.recipe_to_save().steps.is_empty(),
        "pending edits are not saved"
    );
}

#[test]
fn undo_and_redo_move_the_edit_without_recording_it_again() {
    let mut t = tab(&["id", "v"], &[&["1", "x"]]);
    t.table.set(0, 1, CellValue::String("y".into()));
    t.record_hand_edits();
    t.table.undo();
    t.recipe_after_undo();
    assert!(t.recipe.is_empty());
    t.table.redo();
    t.recipe_after_redo();
    assert!(!t.record_hand_edits());
    assert_eq!(t.recipe.len(), 1);
}

#[test]
fn cells_filled_by_an_inserted_column_are_not_typed_values() {
    let mut t = tab(&["id"], &[&["1"]]);
    t.table.insert_column(1, "new".into(), "Utf8".into());
    t.table.set(0, 1, CellValue::String("f".into()));
    assert!(!t.record_hand_edits());
    assert!(t.recipe.is_empty());
}
