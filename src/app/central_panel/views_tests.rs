//! The chart view's buttons, clicked the way a user clicks them.

use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

/// Two years of monthly sales, as a line chart tab.
fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let mut table = DataTable::empty();
    table.columns = vec![
        ColumnInfo {
            name: "month".into(),
            data_type: "Date32".into(),
        },
        ColumnInfo {
            name: "sales".into(),
            data_type: "Float64".into(),
        },
    ];
    table.rows = (0..24)
        .map(|i| {
            let summer = if i % 12 == 5 || i % 12 == 6 {
                10.0
            } else {
                0.0
            };
            vec![
                CellValue::Date(format!("{}-{:02}-01", 2024 + i / 12, i % 12 + 1)),
                CellValue::Float(100.0 + i as f64 + summer),
            ]
        })
        .collect();
    let tab = &mut app.tabs[0];
    tab.filtered_rows = (0..table.row_count()).collect();
    tab.table = table;
    tab.is_chart_tab = true;
    tab.view_mode = ViewMode::Chart;
    tab.chart_config = octa::data::chart::ChartConfig {
        kind: octa::data::chart::ChartKind::Line,
        x_col: Some(0),
        y_cols: vec![1],
        ..Default::default()
    };
    app
}

/// One frame of the active tab's view; every text painted, with its rect.
fn frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(t) => out.push((
                t.galley.text().to_string(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
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
    let mut output = ctx.run_ui(input, |ui| {
        let ctx = ui.ctx().clone();
        app.render_non_table_view(&ctx, ui);
    });
    output.textures_delta.clear();
    let mut out = Vec::new();
    output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

#[test]
fn seasonality_opens_the_numbers_and_a_chart_of_them() {
    let mut app = app();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let texts = frame(&mut app, &ctx, vec![]);
    let label = octa::i18n::t("chart.seasonality_to_table");
    let (_, rect) = texts
        .iter()
        .find(|(t, _)| *t == label)
        .unwrap_or_else(|| panic!("no {label} button: {texts:?}"));
    let at = rect.center();
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        frame(&mut app, &ctx, events);
    }

    assert_eq!(app.tabs.len(), 3, "the chart, the numbers, their chart");
    let numbers = &app.tabs[1];
    assert!(!numbers.is_chart_tab, "a table that can be read and saved");
    assert_eq!(numbers.table.row_count(), 24);
    let names: Vec<&str> = numbers
        .table
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["x", "value", "trend", "seasonal", "rest"]);
    let chart = &app.tabs[2];
    assert!(chart.is_chart_tab, "a real chart tab, which keeps its view");
    assert_eq!(chart.available_view_modes(), [ViewMode::Chart]);
    assert_eq!(chart.chart_config.y_cols, [1, 2, 3, 4]);
    assert_eq!(app.active_tab, 2);
}
