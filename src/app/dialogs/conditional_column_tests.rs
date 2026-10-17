use super::{control_row, field, icon_button, keyword_cell};
use eframe::egui;

/// A combo, a text field, a glyph button and a keyword on one line must
/// be the same height and share one vertical centre.
#[test]
fn every_control_on_a_line_has_one_height_and_centre() {
    let ctx = egui::Context::default();
    let mut rects = Vec::new();
    for _ in 0..2 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            // A theme's padding: egui's stock 1px makes a button shorter
            // than a text field, which no Octa theme does.
            ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
            ui.spacing_mut().interact_size.y = super::control_h(ui);
            let mut v = String::new();
            rects = control_row(ui, |ui| {
                let kw = keyword_cell(ui, 80.0, |ui| ui.label("If"));
                let combo = egui::ComboBox::from_id_salt("c")
                    .selected_text("col")
                    .width(120.0)
                    .show_ui(ui, |_| {})
                    .response;
                let edit = field(ui, 120.0, egui::TextEdit::singleline(&mut v));
                let btn = icon_button(ui, true, "+");
                vec![combo.rect, edit.rect, btn.rect, kw.rect]
            });
        });
        out.textures_delta.clear();
    }
    let (h, cy) = (rects[0].height(), rects[0].center().y);
    for r in &rects[..3] {
        assert!((r.height() - h).abs() < 0.5, "heights differ: {rects:?}");
    }
    for r in &rects {
        assert!((r.center().y - cy).abs() < 0.5, "centres differ: {rects:?}");
    }
}
