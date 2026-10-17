use super::*;

/// Height of the tallest popup egui has open, or 0.0 when none is.
fn open_popup_height(ctx: &egui::Context) -> f32 {
    let layers = ctx.memory(|m| m.areas().visible_layer_ids());
    layers
        .iter()
        .filter(|l| l.order != egui::Order::Background)
        .filter_map(|l| ctx.read_response(l.id.with("move")))
        .fold(0.0_f32, |h, r| h.max(r.rect.height()))
}

/// A submenu that opens while its listing is still loading, then gets 200
/// tables. Driven headlessly near the bottom edge of a short window, the
/// SQL panel's own geometry.
///
/// The regression, and the reason the first two attempts at this menu
/// shipped broken: a menu popup is an auto-sized `Area` that caches its
/// size the first frame it opens, and only ever grows when its content's
/// `min_size` grows. A `ScrollArea` never grows - it shrinks into whatever
/// space it is given, down to its 64px floor. So the popup froze at the
/// size of the "Loading..." label it opened with, and showed two entries
/// forever after. Allocating a box measured from the entries is what makes
/// the content grow, so the popup grows with it.
#[test]
fn a_level_that_opened_while_loading_grows_when_the_listing_lands() {
    let entry = super::super::DbAttachEntry {
        id: "c".to_string(),
        name: "warehouse".to_string(),
        engine: octa::db::DbEngine::Postgres,
        drill: None,
    };
    let loading = super::super::DrillMenu {
        has_catalogs: false,
        nodes: std::collections::HashMap::new(),
    };
    let ready = super::super::DrillMenu {
        has_catalogs: false,
        nodes: std::collections::HashMap::from([(
            Vec::new(),
            super::super::NodeListing::Ready((0..200).map(|i| format!("orders_{i:03}")).collect()),
        )]),
    };
    let ctx = egui::Context::default();
    let pass = |drill: &super::super::DrillMenu, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 300.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let mut action = SqlAction::default();
            ui.add_space(255.0);
            ui.menu_button("attach", |ui| {
                ui.menu_button("warehouse", |ui| {
                    drill_menu(ui, &entry, drill, &mut Vec::new(), &mut action);
                });
            });
        });
        out.textures_delta.clear();
    };
    let click = |pressed: bool| egui::Event::PointerButton {
        pos: egui::pos2(20.0, 270.0),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    // Open the menu, and the connection's submenu inside it, while the
    // listing is still on its way.
    pass(&loading, vec![]);
    pass(
        &loading,
        vec![
            egui::Event::PointerMoved(egui::pos2(20.0, 270.0)),
            click(true),
        ],
    );
    pass(&loading, vec![click(false)]);
    for y in [240.0_f32, 235.0, 245.0, 250.0, 230.0] {
        pass(
            &loading,
            vec![egui::Event::PointerMoved(egui::pos2(30.0, y))],
        );
        pass(&loading, vec![]);
    }
    assert!(
        open_popup_height(&ctx) > 0.0,
        "the submenu should be open while the listing runs"
    );
    // The listing lands.
    for _ in 0..10 {
        pass(&ready, vec![]);
    }
    let grown = open_popup_height(&ctx);
    assert!(
        grown > 200.0,
        "the popup must grow to the list it now holds, got {grown}"
    );
    for _ in 0..30 {
        pass(&ready, vec![]);
    }
    let settled = open_popup_height(&ctx);
    assert!(
        (settled - grown).abs() < 1.0,
        "the popup shrank while it sat open: {grown} -> {settled}"
    );
}
