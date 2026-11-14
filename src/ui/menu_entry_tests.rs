use super::*;

#[test]
fn a_greyed_tip_names_what_and_why() {
    assert_eq!(
        disabled_tip("Opens X.", "Open a table first."),
        "Opens X.\n\nOpen a table first."
    );
    assert_eq!(
        disabled_tip("", "Open a table first."),
        "Open a table first."
    );
}

#[test]
fn a_greyed_entry_is_drawn_but_never_clicks() {
    let ctx = egui::Context::default();
    let mut clicked = true;
    let mut height = 0.0;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(400.0, 300.0),
        )),
        ..Default::default()
    };
    // The font atlas has to be taken, or dropping the output panics.
    let mut full = ctx.run_ui(input, |ui| {
        clicked = menu_entry(
            ui,
            "Chart...",
            "Opens a chart.",
            Some("Open a table first."),
        );
        height = ui.min_rect().height();
    });
    full.textures_delta.clear();
    assert!(!clicked);
    assert!(height > 0.0, "the greyed button must take space");
}

/// Draw the layout entries for `count` tabs and click Grid: what got picked.
fn click_grid(count: usize) -> Option<crate::ui::tiles::TileLayout> {
    let ctx = egui::Context::default();
    let label = crate::i18n::t("tiles.grid");
    let mut picked = None;
    let mut run = |events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(400.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| {
            if let Some(layout) = tile_layout_entries(ui, None, count) {
                picked = Some(layout);
            }
        });
        full.textures_delta.clear();
        full.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() / 2.0),
            _ => None,
        })
    };
    run(vec![]);
    let at = run(vec![]).expect("the entry is drawn, greyed or not");
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run(vec![egui::Event::PointerMoved(at)]);
    run(vec![press(true)]);
    run(vec![press(false)]);
    picked
}

/// With two tabs a grid is side by side under another name, so the entry says
/// so rather than offering a choice that changes nothing.
#[test]
fn grid_is_greyed_until_it_differs_from_side_by_side() {
    assert_eq!(click_grid(2), None);
    assert_eq!(click_grid(3), Some(crate::ui::tiles::TileLayout::Grid));
}
