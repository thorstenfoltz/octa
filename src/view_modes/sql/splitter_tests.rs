//! Unit tests for [`splitter`](splitter). Included via `#[path]` so it stays
//! an inner `tests` module with access to the parent's private items.

use super::*;

fn sum(v: &[f32]) -> f32 {
    v.iter().sum()
}

/// The whole point: the panes ARE the space, so they add up to it exactly.
/// Anything less and the last pane stops short; anything more and it hangs
/// over the pane below, which is the bug this replaced.
#[test]
fn the_panes_always_add_up_to_the_space() {
    for total in [1000.0_f32, 500.0, 280.0, 140.0, 60.0, 1.0] {
        for weights in [
            vec![1.0, 1.0],
            vec![140.0, 180.0, 200.0, 240.0],
            vec![900.0, 10.0, 10.0, 10.0],
            vec![0.0, 0.0, 0.0],
        ] {
            let h = fit(&weights, total, MIN_PANE);
            assert_eq!(h.len(), weights.len());
            assert!(
                (sum(&h) - total).abs() < 0.5,
                "{weights:?} in {total}px summed to {}",
                sum(&h)
            );
            assert!(h.iter().all(|x| *x >= 0.0), "{h:?}");
        }
    }
}

/// Room enough: nobody is below the floor.
#[test]
fn every_pane_keeps_its_floor_while_there_is_room() {
    let h = fit(&[900.0, 10.0, 10.0, 10.0], 1000.0, MIN_PANE);
    assert!(h.iter().all(|x| *x >= MIN_PANE - 0.5), "{h:?}");
    assert!(h[0] > 600.0, "the roomy pane keeps most of it: {h:?}");
}

/// Not enough room for four floors: the floor gives way evenly, rather than
/// the first panes taking theirs and the last getting a sliver outside the
/// panel.
#[test]
fn a_panel_too_short_for_the_floor_shares_evenly() {
    let h = fit(&[140.0, 180.0, 200.0, 240.0], 120.0, MIN_PANE);
    assert!((sum(&h) - 120.0).abs() < 0.5, "{h:?}");
    for x in &h {
        assert!((x - 30.0).abs() < 0.5, "{h:?}");
    }
}

/// A resized window rescales the panes instead of leaving one outside.
#[test]
fn weights_rescale_to_a_new_height() {
    let h = fit(&[100.0, 300.0], 200.0, 0.0);
    assert!(
        (h[0] - 50.0).abs() < 0.5 && (h[1] - 150.0).abs() < 0.5,
        "{h:?}"
    );
}

/// Drive a real drag on the middle handle and check the space moved from one
/// neighbour to the other and nowhere else.
#[test]
fn dragging_a_handle_moves_space_between_its_two_neighbours() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("splitter_under_test");
    let seen: std::cell::RefCell<Vec<egui::Rect>> = Default::default();

    let modifiers = egui::Modifiers::default();
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let run = |events: Vec<egui::Event>| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(600.0, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let rects = vertical_splitter(ui, id, 3);
            *seen.borrow_mut() = rects;
        });
        out.textures_delta.clear();
    };

    run(vec![]);
    run(vec![]);
    let before = seen.borrow().clone();
    assert_eq!(before.len(), 3);
    // The handle between pane 0 and pane 1 sits just under pane 0.
    let handle_y = before[0].bottom() + 3.0;
    let target = handle_y + 120.0;
    let at = egui::pos2(300.0, handle_y);
    let to = egui::pos2(300.0, target);
    run(vec![egui::Event::PointerMoved(at)]);
    run(vec![egui::Event::PointerMoved(at), press(at, true)]);
    run(vec![egui::Event::PointerMoved(to)]);
    run(vec![press(to, false)]);
    run(vec![]);

    let after = seen.borrow().clone();
    assert!(
        after[0].height() > before[0].height() + 100.0,
        "pane 0 grew: {before:?} -> {after:?}"
    );
    assert!(
        after[1].height() < before[1].height() - 100.0,
        "pane 1 gave the space up: {before:?} -> {after:?}"
    );
    assert!(
        (after[2].height() - before[2].height()).abs() < 1.0,
        "pane 2 was not touched: {before:?} -> {after:?}"
    );
    // And still no gaps and no overlap.
    for pair in after.windows(2) {
        assert!(
            pair[0].bottom() <= pair[1].top() + 0.01,
            "panes overlap: {after:?}"
        );
    }
}
