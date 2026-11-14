//! Tests for `panel_fit`. Kept in their own file and pulled back via `#[path]`
//! so they stay an inner `tests` module with access to the private items.

use super::*;

#[test]
fn a_roomy_window_gets_exactly_what_the_panel_asked_for() {
    // 1600px wide: the SQL panel's 440/280 is nowhere near two thirds.
    assert_eq!(clamp(1600.0, 440.0, 280.0), (440.0, 280.0));
}

#[test]
fn a_small_window_keeps_a_third_for_the_table() {
    // The 400px floor. Both the default and the minimum are far too wide.
    let (default_size, min_size) = clamp(400.0, 440.0, 280.0);
    let ceiling = 400.0 * MAX_SHARE;
    assert!(default_size <= ceiling, "{default_size} > {ceiling}");
    assert!(min_size <= ceiling, "{min_size} > {ceiling}");
    assert!(
        400.0 - default_size >= 400.0 / 3.0 - 0.01,
        "the table was left {}px",
        400.0 - default_size
    );
}

#[test]
fn the_default_never_drops_below_the_minimum() {
    // A panel asking for a default smaller than its own minimum (or a clamp
    // that pushes them past each other) must not produce default < min: egui
    // would then resolve the panel to the minimum anyway and the returned
    // default would be a lie.
    for available in [50.0, 120.0, 400.0, 900.0, 4000.0] {
        for (want_default, want_min) in [(440.0, 280.0), (280.0, 440.0), (140.0, 140.0)] {
            let (default_size, min_size) = clamp(available, want_default, want_min);
            assert!(
                default_size >= min_size,
                "available={available} default={default_size} min={min_size}"
            );
        }
    }
}

#[test]
fn a_panel_is_never_widened_beyond_what_it_asked_for() {
    // Clamping only ever shrinks. A huge window must not push a 140px-tall
    // bottom panel out to two thirds of the screen.
    let (default_size, min_size) = clamp(4000.0, 280.0, 140.0);
    assert_eq!((default_size, min_size), (280.0, 140.0));
}

#[test]
fn an_unmeasured_window_passes_the_request_through() {
    // First frame / minimised window: no measurement yet. Clamping against
    // zero would collapse the panel to nothing and it would never come back.
    assert_eq!(clamp(0.0, 440.0, 280.0), (440.0, 280.0));
    assert_eq!(clamp(-1.0, 440.0, 280.0), (440.0, 280.0));
    assert_eq!(clamp(f32::NAN, 440.0, 280.0), (440.0, 280.0));
}

/// The window every panel test below draws into.
fn frame(ctx: &egui::Context, draw: impl FnMut(&mut egui::Ui)) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, draw);
    out.textures_delta.clear();
}

/// What the Assistant's body does: it takes whatever room it is given.
fn fill(ui: &mut egui::Ui) {
    ui.set_min_size(ui.available_size());
}

/// Proves the next test bites: this is what the Assistant panel did, one id
/// for every edge, and it comes out as wide as the window.
#[test]
fn one_id_for_every_edge_inherits_the_other_edges_size() {
    let ctx = egui::Context::default();
    for _ in 0..2 {
        frame(&ctx, |ui| {
            egui::Panel::bottom("shared").resizable(true).show(ui, fill);
        });
    }
    let mut width = 0.0;
    for _ in 0..2 {
        frame(&ctx, |ui| {
            let panel = egui::Panel::right("shared").resizable(true);
            width = panel.show(ui, fill).response.rect.width();
        });
    }
    assert!(width > 1100.0, "the shared id no longer inherits: {width}");
}

#[test]
fn a_panel_moved_to_another_edge_leaves_the_table_its_share() {
    use crate::ui::settings::PanelPosition as P;
    let ctx = egui::Context::default();
    for _ in 0..2 {
        frame(&ctx, |ui| {
            docked("moved", P::Bottom, ui.available_height(), 320.0, 160.0).show(ui, fill);
        });
    }
    for position in [P::Right, P::Left] {
        let (mut width, mut left_over) = (0.0, 0.0);
        for _ in 0..2 {
            frame(&ctx, |ui| {
                let panel = docked("moved", position, ui.available_width(), 720.0, 380.0);
                width = panel.show(ui, fill).response.rect.width();
                left_over = ui.available_width();
            });
        }
        assert!(width <= 1200.0 * MAX_SHARE + 1.0, "{position:?}: {width}");
        assert!(
            left_over >= 390.0,
            "{position:?}: the table got {left_over}"
        );
    }
}

/// A size remembered from a large window must not take a small one whole.
#[test]
fn a_remembered_size_is_held_to_its_share_of_a_smaller_window() {
    use crate::ui::settings::PanelPosition as P;
    let ctx = egui::Context::default();
    let mut width = 0.0;
    for available in [1200.0, 1200.0, 600.0] {
        frame(&ctx, |ui| {
            width = docked("shrunk", P::Right, available, 720.0, 380.0)
                .show(ui, fill)
                .response
                .rect
                .width();
        });
    }
    assert!(width <= 600.0 * MAX_SHARE + 1.0, "{width}");
}
