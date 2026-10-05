use octa::data::{CellValue, SearchMode, ViewMode};
use octa::formats::FormatRegistry;
use octa::formats::write_options::WriteOptions;

use crate::app::state::TabState;

/// A tab holding `text` read from a file called `name`, as a load leaves it.
fn tab_of(dir: &tempfile::TempDir, name: &str, text: &str, view: ViewMode) -> TabState {
    let path = dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    let registry = FormatRegistry::new();
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table = registry
        .reader_for_path(&path)
        .unwrap()
        .read_file(&path)
        .unwrap();
    tab.raw_content = Some(text.to_string());
    tab.raw_content_original = tab.raw_content.clone();
    tab.json_value = serde_json::from_str(text).ok();
    tab.view_mode = view;
    tab.synced_view = (view, tab.compare_mode);
    tab
}

fn switch(tab: &mut TabState, view: ViewMode) {
    tab.view_mode = view;
    tab.sync_views(&FormatRegistry::new(), &WriteOptions::default());
}

fn edit_text(tab: &mut TabState, text: &str) {
    tab.raw_content = Some(text.to_string());
    tab.raw_content_modified = true;
}

#[test]
fn a_table_edit_shows_in_the_raw_text_without_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.csv", "name,n\nann,1\nbob,2\n", ViewMode::Table);
    tab.table.set(1, 0, CellValue::String("eve".into()));

    switch(&mut tab, ViewMode::Raw);

    let raw = tab.raw_content.as_deref().unwrap();
    assert!(raw.contains("eve") && !raw.contains("bob"), "{raw}");
    assert!(
        !tab.raw_content_modified,
        "the table still carries the edit"
    );
    assert!(tab.is_modified());
    assert!(!tab.saves_text());
}

#[test]
fn a_raw_edit_shows_in_the_table_and_saves_as_typed() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.csv", "name,n\nann,1\n", ViewMode::Raw);
    edit_text(&mut tab, "name,n\nann,1\nzoe,3\n");

    switch(&mut tab, ViewMode::Table);

    assert_eq!(tab.view_mode, ViewMode::Table);
    assert_eq!(tab.table.row_count(), 2);
    assert_eq!(tab.table.get(1, 0), Some(&CellValue::String("zoe".into())));
    assert!(tab.filter_dirty);
    assert!(tab.is_modified());
    assert!(
        tab.saves_text(),
        "the text the user typed is what gets saved"
    );
}

#[test]
fn a_table_edit_after_a_raw_edit_wins() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.csv", "name,n\nann,1\n", ViewMode::Raw);
    edit_text(&mut tab, "name,n\nann,1\nzoe,3\n");
    switch(&mut tab, ViewMode::Table);
    tab.table.set(0, 0, CellValue::String("max".into()));
    assert!(!tab.saves_text(), "the table is the newer side");

    switch(&mut tab, ViewMode::Raw);

    let raw = tab.raw_content.as_deref().unwrap();
    assert!(raw.contains("max") && raw.contains("zoe"), "{raw}");
    assert!(!tab.saves_text());
}

#[test]
fn looking_at_the_text_without_editing_it_keeps_the_table() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.csv", "name,n\nann,1\n", ViewMode::Raw);
    edit_text(&mut tab, "name,n\nann,1\nzoe,3\n");
    switch(&mut tab, ViewMode::Table);
    // Not an edit: only a read of the same text again would undo it.
    tab.table.columns[0].data_type = "marker".into();

    switch(&mut tab, ViewMode::Raw);
    switch(&mut tab, ViewMode::Table);

    assert_eq!(tab.table.columns[0].data_type, "marker");
}

#[test]
fn text_that_does_not_read_keeps_the_user_in_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.json", r#"[{"a": 1}]"#, ViewMode::Raw);
    edit_text(&mut tab, r#"[{"a": 1"#);

    switch(&mut tab, ViewMode::Table);

    assert_eq!(tab.view_mode, ViewMode::Raw);
    assert!(tab.parse_error_banner.is_some());
    assert_eq!(tab.table.get(0, 0), Some(&CellValue::Int(1)));
}

#[test]
fn a_partly_loaded_table_does_not_pretend_to_be_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.csv", "name\nann\n", ViewMode::Table);
    tab.table.total_rows = Some(1000);
    tab.table.set(0, 0, CellValue::String("eve".into()));

    switch(&mut tab, ViewMode::Raw);

    assert_eq!(tab.view_mode, ViewMode::Table);
    assert!(tab.parse_error_banner.is_some());
    assert_eq!(tab.raw_content.as_deref(), Some("name\nann\n"));
}

#[test]
fn the_json_tree_follows_a_table_edit() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.json", r#"[{"a": 1}]"#, ViewMode::JsonTree);
    switch(&mut tab, ViewMode::Table);
    tab.table.set(0, 0, CellValue::Int(7));

    switch(&mut tab, ViewMode::JsonTree);

    assert_eq!(tab.view_mode, ViewMode::JsonTree);
    assert_eq!(tab.json_value, Some(serde_json::json!([{"a": 7}])));
}

#[test]
fn the_json_tree_follows_a_raw_edit() {
    let dir = tempfile::tempdir().unwrap();
    let mut tab = tab_of(&dir, "a.json", r#"{"a": 1}"#, ViewMode::Raw);
    edit_text(&mut tab, r#"{"a": 2}"#);

    switch(&mut tab, ViewMode::JsonTree);

    assert_eq!(tab.json_value, Some(serde_json::json!({"a": 2})));
}
