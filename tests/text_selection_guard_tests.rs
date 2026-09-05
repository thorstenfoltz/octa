//! The Ctrl+C arbitration guard, driven headlessly.
//!
//! egui runs without a window, so a real drag-select over a real `Label` can
//! be replayed here: this is what stops the table's clipboard shortcut from
//! stealing Ctrl+C away from a marked chat bubble again.

use eframe::egui;
use octa::ui::text_selection::{copy_pressed, has_active_selection};

/// One pass with the given events. Returns the text egui asked to copy, plus
/// what the guard answered while the pass was running.
fn pass(ctx: &egui::Context, events: Vec<egui::Event>) -> (Vec<String>, bool, bool) {
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(400.0, 200.0),
        )),
        ..Default::default()
    };
    let mut marked = false;
    let mut may_copy = false;
    let mut out = ctx.run_ui(input, |ui| {
        ui.add(egui::Label::new("hello selectable world").selectable(true));
        // Asked after the label rendered, which is where the real callers sit:
        // the chat panel draws before the central panel.
        marked = has_active_selection(ui.ctx());
        may_copy = copy_pressed(ui.ctx());
    });
    // Nothing paints the textures in a test.
    out.textures_delta.clear();
    let copied = out
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    (copied, marked, may_copy)
}

/// Drag across the label, then press Ctrl+C.
fn select_then_copy(ctx: &egui::Context) -> (Vec<String>, bool, bool) {
    pass(ctx, vec![]);
    pass(
        ctx,
        vec![
            egui::Event::PointerMoved(egui::pos2(10.0, 12.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(10.0, 12.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    pass(ctx, vec![egui::Event::PointerMoved(egui::pos2(90.0, 12.0))]);
    pass(
        ctx,
        vec![egui::Event::PointerButton {
            pos: egui::pos2(90.0, 12.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    pass(ctx, vec![egui::Event::Copy])
}

#[test]
fn marked_label_text_owns_the_copy() {
    let ctx = egui::Context::default();
    let (copied, marked, may_copy) = select_then_copy(&ctx);
    assert!(marked, "a dragged-over label counts as marked text");
    assert!(
        !may_copy,
        "a widget with its own Ctrl+C must stand down while text is marked"
    );
    let text = copied.join("|");
    assert!(
        text.contains("ello selectable"),
        "egui must copy the marked text, got {copied:?}"
    );
}

/// The pointer leaving the label does not end the selection: the old
/// pointer-position test in the table's clipboard handler is exactly what this
/// replaces, and it failed as soon as the mouse moved away.
#[test]
fn the_selection_survives_the_pointer_moving_away() {
    let ctx = egui::Context::default();
    select_then_copy(&ctx);
    let (_, marked, _) = pass(
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(5.0, 190.0))],
    );
    assert!(marked, "moving the mouse off the label keeps it marked");
}

/// With nothing marked, Ctrl+C belongs to whoever asked for it (the table).
#[test]
fn an_unmarked_pass_hands_the_copy_back() {
    let ctx = egui::Context::default();
    pass(&ctx, vec![]);
    let (copied, marked, may_copy) = pass(&ctx, vec![egui::Event::Copy]);
    assert!(!marked);
    assert!(may_copy, "no marked text: the widget's own copy may run");
    assert!(copied.is_empty(), "egui copied nothing of its own");
}

/// Clicking away clears the selection, which is how the user hands Ctrl+C back
/// to the table without thinking about it.
#[test]
fn clicking_outside_the_text_releases_the_shortcut() {
    let ctx = egui::Context::default();
    select_then_copy(&ctx);
    let click = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let away = egui::pos2(200.0, 180.0);
    pass(
        &ctx,
        vec![egui::Event::PointerMoved(away), click(away, true)],
    );
    let (_, marked, may_copy) = pass(&ctx, vec![click(away, false), egui::Event::Copy]);
    assert!(!marked, "a click outside the text clears the selection");
    assert!(may_copy, "and the table gets its shortcut back");
}
