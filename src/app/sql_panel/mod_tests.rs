use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

/// A large-file tab holds one page, but its `data` is the whole file: before
/// this, the panel copied the page and answered about 2,000 rows.
#[test]
fn a_large_file_tab_queries_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.csv");
    let mut csv = String::from("id,name\n");
    for i in 0..5000 {
        csv.push_str(&format!("{i},row {i}\n"));
    }
    std::fs::write(&path, csv).unwrap();

    let large = octa::formats::large::open(&path).unwrap();
    let mut tab = TabState::new(octa::data::SearchMode::default());
    tab.table = large.page(0, 2000, None, None).unwrap();
    tab.table.source_path = Some(path.to_string_lossy().to_string());
    tab.large = Some(large);

    ensure_workspace(&mut tab);
    register_active_data(&mut tab).unwrap();
    assert_eq!(data_cell(&mut tab, "SELECT count(*) FROM data"), "5000");
}

fn first_cell(tab: &mut TabState, query: &str) -> String {
    let ws = tab.sql_workspace.as_mut().unwrap();
    ws.execute(query).unwrap().table.rows[0][0].to_string()
}

/// What a Run sees: `data` brought up to date first, then the query.
fn data_cell(tab: &mut TabState, query: &str) -> String {
    register_active_data(tab).unwrap();
    first_cell(tab, query)
}

fn tab_with_names(names: &[&str]) -> TabState {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    tab.table = DataTable {
        columns: vec![ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        }],
        rows: names
            .iter()
            .map(|n| vec![CellValue::String((*n).into())])
            .collect(),
        ..DataTable::empty()
    };
    tab
}

/// Copying millions of rows takes seconds, so a Run copies only after the
/// table changed. `data` is emptied behind the stamp's back here: a Run that
/// still finds it empty did not copy.
#[test]
fn data_is_copied_again_only_once_the_table_changed() {
    let mut tab = tab_with_names(&["a", "b"]);
    ensure_workspace(&mut tab);
    register_active_data(&mut tab).unwrap();
    let ws = tab.sql_workspace.as_mut().unwrap();
    ws.execute("DELETE FROM data").unwrap();
    assert_eq!(data_cell(&mut tab, "SELECT count(*) FROM data"), "0");

    tab.table.set(0, 0, CellValue::String("edited".into()));
    assert_eq!(data_cell(&mut tab, "SELECT min(name) FROM data"), "b");
    assert_eq!(data_cell(&mut tab, "SELECT max(name) FROM data"), "edited");
}

/// An undo followed by a new edit ends on the counts the stamp holds; the
/// per-frame check is what notices the undo in between.
#[test]
fn an_undo_then_a_new_edit_is_seen() {
    let mut tab = tab_with_names(&["a", "b"]);
    tab.table.set(0, 0, CellValue::String("x".into()));
    ensure_workspace(&mut tab);
    register_active_data(&mut tab).unwrap();

    tab.table.undo();
    forget_stale_data(&mut tab);
    tab.table.set(0, 0, CellValue::String("y".into()));
    forget_stale_data(&mut tab);
    assert_eq!(data_cell(&mut tab, "SELECT max(name) FROM data"), "y");
}

/// Removing `data` from the workspace list must not leave a Run without it.
#[test]
fn a_removed_data_table_comes_back() {
    let mut tab = tab_with_names(&["a"]);
    ensure_workspace(&mut tab);
    register_active_data(&mut tab).unwrap();
    tab.sql_workspace
        .as_mut()
        .unwrap()
        .remove_table("data")
        .unwrap();
    assert_eq!(data_cell(&mut tab, "SELECT count(*) FROM data"), "1");
}

/// A Run copies `data` only when it is missing or the table changed since.
#[test]
fn a_run_knows_when_data_needs_copying() {
    let mut tab = tab_with_names(&["a"]);
    ensure_workspace(&mut tab);
    assert!(job::data_is_stale(&tab), "never copied");
    register_active_data(&mut tab).unwrap();
    assert!(!job::data_is_stale(&tab));
    tab.table.set(0, 0, CellValue::String("b".into()));
    assert!(job::data_is_stale(&tab), "edited since");
}
