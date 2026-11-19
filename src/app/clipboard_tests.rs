use octa::ui::settings::{AppSettings, IconVariant};

use crate::app::state::OctaApp;

fn app(zoom_percent: u32) -> OctaApp {
    OctaApp::new(
        vec![],
        AppSettings {
            zoom_percent,
            ..AppSettings::default()
        },
        IconVariant::Rose,
    )
}

#[test]
fn octa_opens_at_the_zoom_it_was_left_at() {
    assert_eq!(app(150).zoom_percent, 150);
    assert_eq!(app(100).zoom_percent, 100);
    assert_eq!(app(9000).zoom_percent, 500, "a hand-edited settings.toml");
}

// `set_zoom` is `remember_zoom` plus a save, which no test may run: a save here
// writes the developer's real settings.toml. What it would write is checked below.
#[test]
fn a_new_zoom_is_what_the_next_start_reads() {
    let mut app = app(100);
    app.remember_zoom(130);
    assert_eq!(app.zoom_percent, 130);
    let written = toml::to_string(&app.settings).unwrap();
    let next: AppSettings = toml::from_str(&written).unwrap();
    assert_eq!(next.zoom_percent(), 130);

    app.remember_zoom(9000);
    assert_eq!(app.zoom_percent, 500);
    assert_eq!(app.settings.zoom_percent(), 500);
}

fn cell(app: &OctaApp, r: usize, c: usize) -> String {
    app.tabs[app.active_tab]
        .table
        .get(r, c)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn shape(app: &OctaApp) -> (usize, usize) {
    let t = &app.tabs[app.active_tab].table;
    (t.row_count(), t.col_count())
}

/// File > New table on a fresh start used to leave the blank tab Octa starts
/// with beside the new one: two "Untitled" tabs.
#[test]
fn a_new_table_takes_the_blank_tab_octa_starts_with() {
    let mut app = app(100);
    assert_eq!(app.tabs.len(), 1);
    app.open_new_table_tab(3, 1);
    assert_eq!(app.tabs.len(), 1, "no stray Untitled beside it");
    assert_eq!(shape(&app), (1, 3));
    // The first one holds a table now, so a second goes beside it.
    app.open_new_table_tab(2, 2);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(shape(&app), (2, 2));
}

#[test]
fn a_paste_bigger_than_the_table_grows_it_and_undoes_in_one_step() {
    let mut app = app(100);
    app.open_new_table_tab(3, 1);
    app.paste_text_into_table("a\tb\tc\td\te\n1\t2\t3\t4\t5\n6\t7\t8\t9\t10\n11\t12\t13\t14\t15\n");
    assert_eq!(shape(&app), (4, 5));
    let names: Vec<_> = app.tabs[0]
        .table
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["col1", "col2", "col3", "col4", "col5"]);
    assert_eq!(cell(&app, 0, 4), "e");
    assert_eq!(cell(&app, 3, 4), "15");

    assert!(app.tabs[0].table.undo());
    assert_eq!(shape(&app), (1, 3), "one Ctrl+Z takes the whole paste back");
    assert_eq!(cell(&app, 0, 0), "");
}

#[test]
fn a_paste_smaller_than_the_table_leaves_the_rest_empty() {
    let mut app = app(100);
    app.open_new_table_tab(4, 3);
    app.paste_text_into_table("x\ty\n");
    assert_eq!(shape(&app), (3, 4));
    assert_eq!(
        (cell(&app, 0, 0), cell(&app, 0, 1)),
        ("x".into(), "y".into())
    );
    assert_eq!(
        (cell(&app, 0, 2), cell(&app, 1, 0)),
        (String::new(), String::new())
    );
}

/// Rows added at the end of what is loaded would land in the middle of the
/// table, and new columns on a database tab would be a schema change.
#[test]
fn a_paste_stops_at_the_edge_while_rows_are_still_coming_or_on_a_database_tab() {
    let block = "a\tb\tc\n1\t2\t3\n";
    let mut loading = app(100);
    loading.open_new_table_tab(2, 1);
    loading.tabs[0].bg_can_load_more = true;
    loading.paste_text_into_table(block);
    assert_eq!(shape(&loading), (1, 2));
    assert_eq!(cell(&loading, 0, 1), "b");

    let mut partial = app(100);
    partial.open_new_table_tab(2, 1);
    partial.tabs[0].table.total_rows = Some(50);
    partial.paste_text_into_table(block);
    assert_eq!(shape(&partial), (1, 2));

    let mut database = app(100);
    database.open_new_table_tab(2, 1);
    database.tabs[0].db_origin = Some(crate::app::state::DbOrigin {
        conn_id: "c".into(),
        catalog: None,
        schema: "s".into(),
        table: "t".into(),
        identity: None,
    });
    database.paste_text_into_table(block);
    assert_eq!(shape(&database), (1, 2));
}
