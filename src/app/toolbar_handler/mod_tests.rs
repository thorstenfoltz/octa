use octa::data::{CellValue, ColumnInfo, DataTable, ViewMode};
use octa::i18n::t;

use crate::app::state::OctaApp;

fn table(format: &str, columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        format_name: Some(format.to_string()),
        columns: columns
            .iter()
            .map(|(name, data_type)| ColumnInfo {
                name: (*name).into(),
                data_type: (*data_type).into(),
            })
            .collect(),
        rows,
        ..DataTable::empty()
    }
}

/// One frame of the toolbar: every text painted, with where it is.
fn step(
    app: &mut OctaApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(t) => {
                out.push((t.galley.text().to_owned(), t.visual_bounding_rect()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.render_toolbar(ui));
    output.textures_delta.clear();
    let mut texts = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, &mut texts));
    texts
}

fn click_on(app: &mut OctaApp, ctx: &egui::Context, text: &str, texts: &[(String, egui::Rect)]) {
    let at = texts
        .iter()
        .find(|(t, _)| t == text)
        .map(|(_, r)| r.center())
        .unwrap_or_else(|| panic!("{text:?} not drawn in {texts:?}"));
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    step(app, ctx, vec![egui::Event::PointerMoved(at)]);
    step(app, ctx, vec![button(true)]);
    step(app, ctx, vec![button(false)]);
}

/// Open the View menu and click Map, as a person does, then say whether the
/// tab went there. A greyed entry takes no click.
fn view_map_reaches_the_map(table: DataTable) -> bool {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs[0].table = table;
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = step(&mut app, &ctx, vec![]);
    }
    click_on(&mut app, &ctx, &t("menu.view"), &texts);
    for _ in 0..2 {
        texts = step(&mut app, &ctx, vec![]);
    }
    click_on(&mut app, &ctx, &t("view_menu.map"), &texts);
    app.tabs[0].view_mode == ViewMode::Map
}

fn text(s: &str) -> CellValue {
    CellValue::String(s.into())
}

/// The user's report: a file naming countries left View > Map greyed, while F4
/// reached the map. The menu asked "is this GeoJSON?" and the tab knew better.
#[test]
fn view_map_opens_for_a_table_of_countries() {
    let rows = ["Germany", "France", "Spain", "Japan"]
        .map(|c| vec![text(c), CellValue::Int(1)])
        .to_vec();
    let countries = table("CSV", &[("country", "Utf8"), ("n", "Int64")], rows);
    assert!(view_map_reaches_the_map(countries));
}

#[test]
fn view_map_opens_for_a_table_of_coordinates() {
    let rows = vec![
        vec![CellValue::Float(52.5), CellValue::Float(13.4)],
        vec![CellValue::Float(48.8), CellValue::Float(2.3)],
    ];
    let points = table("CSV", &[("lat", "Float64"), ("lon", "Float64")], rows);
    assert!(view_map_reaches_the_map(points));
}

#[test]
fn view_map_opens_for_a_shapefile_and_a_geojson_file() {
    let rows = vec![vec![text("POINT(1 2)")]];
    let columns = [("__geometry", "Utf8")];
    assert!(view_map_reaches_the_map(table(
        "Shapefile",
        &columns,
        rows.clone()
    )));
    assert!(view_map_reaches_the_map(table("GeoJSON", &columns, rows)));
}

#[test]
fn view_map_stays_greyed_for_a_table_with_nothing_to_plot() {
    let rows = vec![vec![text("apple")], vec![text("pear")], vec![text("plum")]];
    let fruit = table("CSV", &[("fruit", "Utf8")], rows);
    assert!(!view_map_reaches_the_map(fruit));
}
