//! The write-back dialog driven headlessly: one frame paints it, the Write
//! button is found by its painted label and clicked like a user would.

use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn frame(
    ctx: &egui::Context,
    app: &mut OctaApp,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| render_sql_write_back_dialog(app, ui.ctx()));
    out.textures_delta.clear();
    out.shapes
}

/// Where the text `want` was painted.
fn painted_at(shapes: &[egui::epaint::ClippedShape], want: &str) -> Option<egui::Pos2> {
    fn walk(s: &egui::Shape, want: &str) -> Option<egui::Pos2> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == want => {
                Some(t.visual_bounding_rect().center())
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| walk(s, want)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, want))
}

fn click_write(app: &mut OctaApp) {
    let ctx = egui::Context::default();
    frame(&ctx, app, vec![]);
    let shapes = frame(&ctx, app, vec![]);
    let pos = painted_at(&shapes, &octa::i18n::t("dialog.swb_write")).expect("Write button");
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(&ctx, app, vec![egui::Event::PointerMoved(pos)]);
    frame(&ctx, app, vec![button(true)]);
    frame(&ctx, app, vec![button(false)]);
}

fn two_rows() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "operation".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![
        vec![CellValue::String("read".into())],
        vec![CellValue::String("write".into())],
    ];
    t
}

fn app() -> (OctaApp, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    (app, dir)
}

fn rows_in(path: &std::path::Path, table: &str) -> i64 {
    duckdb::Connection::open(path)
        .and_then(|c| {
            c.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
                r.get(0)
            })
        })
        .unwrap()
}

#[test]
fn a_result_the_server_returned_goes_into_a_new_file() {
    // Ran on Databricks: the tables it names do not exist in the local
    // workspace, so running it again there can only fail.
    let (mut app, dir) = app();
    app.tabs[0].sql_workspace = Some(octa::sql::SqlWorkspace::new().unwrap());
    app.tabs[0].sql.result = Some(two_rows());
    app.tabs[0].sql.last_query = "select operation from adp.`fabric-ingest`.eventhistory".into();
    app.tabs[0].sql.last_query_on_server = true;
    app.open_sql_write_back_dialog();
    let target = dir.path().join("adp.duckdb");
    let st = app.tabs[0].sql_write_back.as_mut().unwrap();
    st.target_path = target.clone();
    st.kind = WriteBackKind::from_path(&target);
    st.table = "adp".into();
    click_write(&mut app);
    assert!(
        app.tabs[0].sql_write_back.is_none(),
        "dialog still open: {:?}",
        app.tabs[0]
            .sql_write_back
            .as_ref()
            .and_then(|s| s.error.clone())
    );
    assert_eq!(rows_in(&target, "adp"), 2);
}

#[test]
fn a_local_result_still_runs_its_query_and_a_failure_says_why() {
    let (mut app, dir) = app();
    let mut ws = octa::sql::SqlWorkspace::new().unwrap();
    ws.set_active_table(&two_rows()).unwrap();
    app.tabs[0].sql_workspace = Some(ws);
    app.tabs[0].sql.result = Some(two_rows());
    app.tabs[0].sql.last_query = "SELECT * FROM no_such_table".into();
    app.open_sql_write_back_dialog();
    let target = dir.path().join("local.duckdb");
    let st = app.tabs[0].sql_write_back.as_mut().unwrap();
    st.target_path = target.clone();
    st.kind = WriteBackKind::from_path(&target);
    click_write(&mut app);
    let error = app.tabs[0]
        .sql_write_back
        .as_ref()
        .and_then(|s| s.error.clone())
        .expect("the dialog stays open with the reason");
    // The server's reason, not only "writing result to ...".
    assert!(error.contains("no_such_table"), "{error}");
}

#[test]
fn the_dialog_says_whether_write_adds_to_a_file_or_makes_one() {
    let (mut app, dir) = app();
    app.tabs[0].table = two_rows();
    app.open_table_to_db_dialog();
    let painted = |app: &mut OctaApp, path: &std::path::Path| {
        let st = app.tabs[0].sql_write_back.as_mut().unwrap();
        st.target_path = path.to_path_buf();
        let ctx = egui::Context::default();
        frame(&ctx, app, vec![]);
        frame(&ctx, app, vec![])
    };
    let new = octa::i18n::t("dialog.swb_target_new");
    let existing = octa::i18n::t("dialog.swb_target_exists");

    let shapes = painted(&mut app, &dir.path().join("fresh.duckdb"));
    assert!(painted_at(&shapes, &new).is_some());
    assert!(painted_at(&shapes, &existing).is_none());

    let there = dir.path().join("there.duckdb");
    duckdb::Connection::open(&there).unwrap();
    let shapes = painted(&mut app, &there);
    assert!(painted_at(&shapes, &existing).is_some());
}
