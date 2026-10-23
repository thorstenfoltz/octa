use super::*;

/// Every table of a page opens in its own tab, named after the file and the
/// table's caption. A reader that leaves `source_path` unset (HTML, PDF) still
/// gives its tabs the file: before, they read "Untitled - Products" and Save
/// and Refresh had no file to go back to.
#[test]
fn each_table_of_a_page_opens_in_a_tab_named_after_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("menu.html");
    std::fs::write(
        &path,
        "<table><caption>Products</caption><tr><th>id</th></tr><tr><td>1</td></tr></table>\
         <table><tr><th>city</th></tr><tr><td>Rome</td></tr></table>",
    )
    .unwrap();
    let mut app = OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    );

    app.load_file(path.clone());

    let titles: Vec<String> = app.tabs.iter().map(|t| t.title_display()).collect();
    assert_eq!(titles, ["menu.html - Products", "menu.html - Table 2"]);
    let file = path.to_string_lossy().to_string();
    assert!(
        app.tabs
            .iter()
            .all(|t| t.table.source_path.as_deref() == Some(file.as_str()))
    );
}
