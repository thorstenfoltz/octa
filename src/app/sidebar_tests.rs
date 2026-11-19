use super::*;

/// Draws two browsers stacked as the sidebar does and returns the first one's
/// height. `events` arrive in this frame.
fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> f32 {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(300.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut first = 0.0;
    let mut out = ctx.run_ui(input, |ui| {
        stacked_section(ui, egui::Id::new("first"), 2, |ui| {
            first = ui.max_rect().height();
        });
        stacked_section(ui, egui::Id::new("second"), 1, |_| {});
    });
    out.textures_delta.clear();
    first
}

/// Cloud and databases on one edge used to get a fixed half each, with a
/// plain line between them that could not be moved (user, 2026-11-18).
#[test]
fn the_line_between_two_stacked_browsers_can_be_dragged() {
    let ctx = egui::Context::default();
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };

    let before = frame(&ctx, vec![]);
    assert!(
        (before - 300.0).abs() < 2.0,
        "an even share to start: {before}"
    );
    let line = egui::pos2(150.0, before);
    frame(
        &ctx,
        vec![egui::Event::PointerMoved(line), press(line, true)],
    );
    let to = egui::pos2(150.0, before + 150.0);
    frame(&ctx, vec![egui::Event::PointerMoved(to)]);
    frame(&ctx, vec![press(to, false)]);
    let after = frame(&ctx, vec![]);

    assert!(
        (after - (before + 150.0)).abs() < 4.0,
        "the drag moves the line: {before} -> {after}"
    );
}
