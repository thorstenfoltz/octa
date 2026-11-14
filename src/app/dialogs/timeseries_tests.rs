//! The dialog drawn headless: its pickers explain themselves on hover.

use super::*;
use octa::data::{CellValue, ColumnInfo};

/// Paint frames with the pointer resting at `at` (if any), advancing time so
/// a tooltip can appear; returns every painted text.
fn hover_texts(
    ctx: &egui::Context,
    at: Option<egui::Pos2>,
    mut draw: impl FnMut(&egui::Context),
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
    let mut out = Vec::new();
    // egui's clock must only move forward, across calls too.
    let start = ctx.input(|i| i.time) + 1.0;
    for step in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            time: Some(start + step as f64),
            // Moved once, then resting: a move every frame restarts egui's
            // tooltip delay.
            events: at
                .filter(|_| step == 0)
                .map(|p| vec![egui::Event::PointerMoved(p)])
                .unwrap_or_default(),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| draw(ui.ctx()));
        output.textures_delta.clear();
        out.clear();
        output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    }
    out
}

#[test]
fn the_time_column_picker_has_a_tooltip() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let table = &mut app.tabs[0].table;
    table.columns = [("day", "Date32"), ("sales", "Float64")]
        .map(|(n, d)| ColumnInfo {
            name: n.into(),
            data_type: d.into(),
        })
        .to_vec();
    table.rows = vec![vec![
        CellValue::Date("2024-01-01".into()),
        CellValue::Float(5.0),
    ]];
    app.timeseries_dialog = Some(crate::app::state::TimeseriesState::default());
    let ctx = egui::Context::default();
    let texts = hover_texts(&ctx, None, |ctx| render_timeseries_dialog(&mut app, ctx));
    let pick = octa::i18n::t("dialog.pv_pick");
    let (_, rect) = texts
        .iter()
        .find(|(t, _)| *t == pick)
        .unwrap_or_else(|| panic!("no picker: {texts:?}"))
        .clone();
    let texts = hover_texts(&ctx, Some(rect.center()), |ctx| {
        render_timeseries_dialog(&mut app, ctx)
    });
    let hint = octa::i18n::t("timeseries.time_col_hint");
    assert!(texts.iter().any(|(t, _)| *t == hint), "{texts:?}");
}
