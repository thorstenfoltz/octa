use super::*;

#[test]
fn arrows_step_one_place_and_stop_at_the_ends() {
    assert_eq!(step_target(2, 5, true), Some(1));
    assert_eq!(step_target(2, 5, false), Some(3));
    assert_eq!(step_target(0, 5, true), None);
    assert_eq!(step_target(4, 5, false), None);
    // A one-column table moves nowhere.
    assert_eq!(step_target(0, 1, true), None);
    assert_eq!(step_target(0, 1, false), None);
}

/// The panel must fill its dock or egui persists the body's rect and the
/// panel snaps back to its content on the next frame. This pins the one
/// line that prevents it, the same defect fixed for the SQL pane - but on
/// a left/right dock the axis egui needs filled is *width*, not height
/// (egui's own `Panel` already fills the cross axis for us; see
/// `containers/panel.rs`'s `show_inside_dyn`, which sets the content
/// ui's min height to the full cross-axis extent and its min width to
/// only the panel's *minimum* size, so a body narrower than the dragged
/// width silently reports that narrower width back and the panel snaps
/// to it next frame).
#[test]
fn navigator_panel_keeps_a_dragged_width() {
    let ctx = egui::Context::default();
    let width = std::cell::Cell::new(0.0_f32);
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let resp = egui::Panel::left("octa_column_navigator")
                .resizable(true)
                .default_size(260.0)
                .min_size(160.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label("one short row");
                });
            width.set(resp.response.rect.width());
        });
        out.textures_delta.clear();
    }
    assert!(
        (width.get() - 260.0).abs() < 8.0,
        "panel settled at {}px, expected its 260px default",
        width.get()
    );
}
