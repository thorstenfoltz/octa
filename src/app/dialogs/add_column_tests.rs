use super::*;
use octa::data::{CellValue, ColumnInfo};

/// One Ctrl+Z takes the whole formula column back, not one cell of it.
#[test]
fn a_formula_column_is_one_undo_step() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let tab = &mut app.tabs[0];
    tab.table.columns = ["a", "b"]
        .map(|n| ColumnInfo {
            name: n.into(),
            data_type: "Int64".into(),
        })
        .to_vec();
    tab.table.rows = (1..=3)
        .map(|i| vec![CellValue::Int(i), CellValue::Int(10 * i)])
        .collect();
    tab.new_col_name = "sum".into();
    tab.new_col_type = "Int64".into();
    tab.new_col_formula = "=A1+B1".into();
    tab.insert_col_at = None;

    add_formula_column(&mut app);
    let table = &app.tabs[0].table;
    assert_eq!(table.columns[2].name, "sum");
    assert_eq!(table.get(2, 2), Some(&CellValue::Int(33)));

    app.tabs[0].table.undo();
    assert_eq!(app.tabs[0].table.col_count(), 2);
}
