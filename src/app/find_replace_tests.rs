use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn tab() -> TabState {
    let mut tab = TabState::new(data::SearchMode::Plain);
    let mut t = DataTable::empty();
    t.columns = ["city", "note", "secret"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".into(),
        })
        .collect();
    let s = |v: &str| CellValue::String(v.into());
    t.rows = vec![
        vec![s("Aachen"), s("near aachen"), s("aachen")],
        vec![s("Bonn"), s("-"), s("aachen")],
        vec![s("aachen"), s("x"), s("x")],
    ];
    tab.table = t;
    tab.filtered_rows = vec![0, 1, 2];
    tab
}

/// The box finds in every column the user sees, whatever the toolbar
/// search's column scope says, in the rows on screen and in display order;
/// a hidden column is left alone, since its change could not be seen.
#[test]
fn the_box_finds_every_visible_column_of_the_rows_on_screen() {
    let mut tab = tab();
    tab.search_scope_col = Some(1);
    tab.hidden_columns.insert(2);
    tab.filtered_rows = vec![2, 0];
    let input = find_box::FindInput {
        query: "AACHEN".into(),
        ..Default::default()
    };
    let cells = box_cells(&tab, &find_box::matcher(&input));
    assert_eq!(cells, vec![(2, 0), (0, 0), (0, 1)]);
}
