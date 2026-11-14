//! The Store asks for an in-product way to report generated output. Classify
//! with Jev shows a model's labels, so its route to TypeSafe AI must be on
//! screen in the dialog, with nothing switched on, and reachable from the
//! Classify dialog.

use super::*;

fn app() -> OctaApp {
    OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    )
}

/// Every text painted over a few frames, with where its widget sits.
fn painted(
    app: &mut OctaApp,
    events: Vec<egui::Event>,
    ctx: &egui::Context,
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
            egui::vec2(1200.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        super::super::classify::render_classify_dialog(app, ui.ctx());
        render_ai_report_dialog(app, ui.ctx());
    });
    output.textures_delta.clear();
    let mut out = Vec::new();
    output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

#[test]
fn the_report_dialog_names_typesafe_for_jev_labels() {
    let mut app = app();
    assert!(!app.settings.jev.enabled, "shown with Jev switched off");
    app.show_ai_report_dialog = true;
    let ctx = egui::Context::default();
    painted(&mut app, vec![], &ctx);
    let texts = painted(&mut app, vec![], &ctx);
    let has = |needle: &str| texts.iter().any(|(t, _)| t.contains(needle));
    assert!(has("TypeSafe AI's Jev model"), "{texts:?}");
    assert!(has("Report to TypeSafe AI"), "{texts:?}");
    assert!(octa::api::jev::REPORT_URL.starts_with("https://typesafe.ai/"));
}

#[test]
fn the_classify_dialog_has_a_report_button_that_opens_it() {
    let mut app = app();
    // Set directly: the menu entry opens it only once Jev is switched on.
    app.tabs[0].table.columns = vec![octa::data::ColumnInfo {
        name: "msg".into(),
        data_type: "Utf8".into(),
    }];
    app.classify_dialog = Some(Default::default());
    let ctx = egui::Context::default();
    painted(&mut app, vec![], &ctx);
    let texts = painted(&mut app, vec![], &ctx);
    let (_, rect) = texts
        .iter()
        .find(|(t, _)| t == "Report")
        .unwrap_or_else(|| panic!("no Report button: {texts:?}"));
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
        painted(&mut app, events, &ctx);
    }
    assert!(app.show_ai_report_dialog);
}
