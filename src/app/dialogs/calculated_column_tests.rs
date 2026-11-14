//! The name you typed is the name you get, or the dialog says why not.

use octa::data::{CellValue, ColumnInfo, DataTable};

use super::render_calculated_column_dialog;
use crate::app::state::OctaApp;

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
    let mut out = ctx.run_ui(input, |ui| render_calculated_column_dialog(app, ui.ctx()));
    out.textures_delta.clear();
    out.shapes
}

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

fn app_with_name(name: &str) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = vec![vec![CellValue::Int(1), CellValue::Int(10)]];
    app.tabs[0].table = t;
    app.open_calculated_column();
    let st = app.calculated_column_dialog.as_mut().unwrap();
    st.spec.value = Some(1);
    st.name = name.to_string();
    st.name_edited = true;
    app
}

/// Presses Add column, and returns what was on screen just before.
fn press_add(app: &mut OctaApp) -> Vec<egui::epaint::ClippedShape> {
    let ctx = egui::Context::default();
    frame(&ctx, app, vec![]);
    let shapes = frame(&ctx, app, vec![]);
    let pos = painted_at(&shapes, &octa::i18n::t("calc.apply")).expect("Add column button");
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(&ctx, app, vec![egui::Event::PointerMoved(pos)]);
    frame(&ctx, app, vec![button(true)]);
    frame(&ctx, app, vec![button(false)]);
    shapes
}

#[test]
fn a_taken_name_adds_nothing_and_says_why() {
    let mut app = app_with_name("amount");
    let shapes = press_add(&mut app);
    assert_eq!(app.tabs[0].table.col_count(), 2);
    assert!(
        painted_at(&shapes, &octa::i18n::t("hashcols.name_taken")).is_some(),
        "the reason is on screen"
    );
}

#[test]
fn a_free_name_is_added_as_typed() {
    let mut app = app_with_name("place");
    let shapes = press_add(&mut app);
    assert!(app.tabs[0].table.columns.iter().any(|c| c.name == "place"));
    assert!(painted_at(&shapes, &octa::i18n::t("hashcols.name_taken")).is_none());
}
