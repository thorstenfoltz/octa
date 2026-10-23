use super::*;

const NOTEBOOK: &str = r#"{"cells": [{"cell_type": "code", "source": ["print(1)"], "metadata": {}, "outputs": [], "execution_count": null}], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#;

fn app_with(dir: &tempfile::TempDir) -> (OctaApp, std::path::PathBuf) {
    let path = dir.path().join("a.ipynb");
    std::fs::write(&path, NOTEBOOK).unwrap();
    let mut app = OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    );
    app.load_file(path.clone());
    (app, path)
}

/// A notebook opens in its Notebook view and, being a JSON file, also offers
/// the JSON tree. The flat cell table is no view of its own.
#[test]
fn a_notebook_opens_as_a_notebook_with_a_json_tree_and_no_table_view() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = app_with(&dir);

    let tab = &app.tabs[app.active_tab];
    assert_eq!(tab.view_mode, ViewMode::Notebook);
    let modes = tab.available_view_modes();
    assert!(modes.contains(&ViewMode::JsonTree), "{modes:?}");
    assert!(!modes.contains(&ViewMode::Table), "{modes:?}");
}

/// Metadata changed in the JSON tree, then a cell edited in the Notebook
/// view: Save writes both, not the cell into the notebook as it was on disk.
#[test]
fn saving_a_cell_edit_keeps_metadata_changed_in_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with(&dir);
    let opts = formats::write_options::WriteOptions::default();
    let i = app.active_tab;
    app.tabs[i].view_mode = ViewMode::JsonTree;
    app.tabs[i].sync_views(&app.registry, &opts);
    let mut edited = app.tabs[i].json_value.clone().unwrap();
    edited["metadata"]["kernelspec"] = serde_json::json!({"name": "python3"});
    app.tabs[i].raw_content = Some(serde_json::to_string_pretty(&edited).unwrap());
    app.tabs[i].raw_content_modified = true;
    app.tabs[i].view_mode = ViewMode::Notebook;
    app.tabs[i].sync_views(&app.registry, &opts);
    app.tabs[i]
        .table
        .set(0, 2, data::CellValue::String("print(2)".into()));

    app.do_save_tab_inner(
        i,
        path.clone(),
        crate::app::state::SaveMode::Rebase,
        None,
        None,
        None,
    );

    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["cells"][0]["source"][0], "print(2)");
    assert_eq!(saved["metadata"]["kernelspec"]["name"], "python3");
    assert!(!app.tabs[i].is_modified());
}
