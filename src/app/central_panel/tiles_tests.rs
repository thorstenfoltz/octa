use super::*;
use crate::app::state::TabState;
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::ui::tiles::TileLayout;

/// Two tabs of 400 rows, `alpha` and `beta`, tiled side by side, `alpha`
/// active.
fn tiled_app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for name in ["alpha", "beta"] {
        let mut t = TabState::new(app.settings.default_search_mode);
        let mut table = DataTable::empty();
        table.columns = (0..4)
            .map(|c| ColumnInfo {
                name: format!("col{c}"),
                data_type: "Utf8".into(),
            })
            .collect();
        table.rows = (0..400)
            .map(|r| {
                (0..4)
                    .map(|c| CellValue::String(format!("r{r}c{c}")))
                    .collect()
            })
            .collect();
        t.table = table;
        t.custom_tab_label = Some(name.to_string());
        app.tabs.push(t);
    }
    app.tile_tabs(&[0, 1], TileLayout::SideBySide);
    app.active_tab = 0;
    app
}

/// One frame of the central panel on a 900 x 600 screen; returns every text
/// it painted, with where.
fn frame(
    ctx: &egui::Context,
    app: &mut OctaApp,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Pos2)> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_central_panel(ui));
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.pos)),
            _ => None,
        })
        .collect()
}

/// A point well inside the right-hand tile, `beta`.
const IN_BETA: egui::Pos2 = egui::pos2(700.0, 300.0);

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    button_of(egui::PointerButton::Primary, pos, pressed)
}

fn button_of(button: egui::PointerButton, pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Default::default(),
    }
}

#[test]
fn each_tile_draws_a_strip_naming_its_tab() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    frame(&ctx, &mut app, vec![]);
    let texts = frame(&ctx, &mut app, vec![]);
    for name in ["alpha", "beta"] {
        assert_eq!(
            texts.iter().filter(|(s, _)| s == name).count(),
            1,
            "one strip names {name}: {texts:?}"
        );
    }
    assert_eq!(app.active_tab, 0, "drawing the tiles keeps the focus");
}

#[test]
fn a_press_in_another_tile_focuses_it() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(IN_BETA)]);
    frame(&ctx, &mut app, vec![button(IN_BETA, true)]);
    frame(&ctx, &mut app, vec![button(IN_BETA, false)]);
    assert_eq!(app.active_tab, 1, "beta is focused now");
    assert_eq!(app.active_tile_members().map(|m| m.1), Some(vec![0, 1]));
}

#[test]
fn a_secondary_press_in_another_tile_focuses_it() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(IN_BETA)]);
    let right = egui::PointerButton::Secondary;
    frame(&ctx, &mut app, vec![button_of(right, IN_BETA, true)]);
    frame(&ctx, &mut app, vec![button_of(right, IN_BETA, false)]);
    assert_eq!(app.active_tab, 1, "a right-click focuses beta too");
}

#[test]
fn the_wheel_scrolls_the_tile_under_the_pointer_not_the_focused_one() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(IN_BETA)]);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            modifiers: Default::default(),
        }],
    );
    frame(&ctx, &mut app, vec![]);
    assert!(
        app.tabs[1].table_state.scroll_y() > 0.0,
        "beta, under the pointer, scrolls"
    );
    assert_eq!(app.tabs[0].table_state.scroll_y(), 0.0, "alpha stays put");
    assert_eq!(app.active_tab, 0, "the wheel does not move the focus");
}

#[test]
fn keys_go_to_the_focused_tile_only() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    for t in &mut app.tabs {
        t.table_state.selected_cell = Some((0, 0));
    }
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(IN_BETA)]);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert_eq!(app.tabs[0].table_state.selected_cell, Some((1, 0)));
    assert_eq!(
        app.tabs[1].table_state.selected_cell,
        Some((0, 0)),
        "the tile under the pointer is not the focused one"
    );
}

#[test]
fn a_dragged_cut_stays_between_its_neighbours() {
    // Three columns of 100 px: cuts at 100 and 200 of 300.
    let sizes = [100.0, 100.0, 100.0];
    let f = moved_cut(&sizes, 0, 150.0);
    assert_eq!(f, vec![0.5, 200.0 / 300.0]);
    let f = moved_cut(&sizes, 0, 260.0);
    assert_eq!(f[0], f[1], "the first cut stops at the second");
    let f = moved_cut(&sizes, 1, -50.0);
    assert_eq!(f[1], f[0], "the second stops at the first");
}

#[test]
fn a_tile_that_is_not_focused_cannot_take_the_keyboard_focus() {
    // beta's Record view opens an edit field and asks for the focus, as a
    // JSON tree edit does every frame.
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    let beta = &mut app.tabs[1];
    beta.view_mode = octa::data::ViewMode::Record;
    beta.table_state.selected_cell = Some((0, 0));
    beta.table_state.editing_cell = Some((0, 0, "r0c0".into()));
    beta.table_state.edit_needs_focus = true;
    frame(&ctx, &mut app, vec![]);
    assert!(
        !app.tabs[1].table_state.edit_needs_focus,
        "the field was drawn and asked"
    );
    assert_eq!(ctx.memory(|m| m.focused()), None, "and was given nothing");
}

/// The middle of the line left of `beta`'s tile, read off its strip.
fn line_left_of_beta(ctx: &egui::Context, app: &mut OctaApp) -> egui::Pos2 {
    let (_, at) = frame(ctx, app, vec![])
        .into_iter()
        .find(|(s, _)| s == "beta")
        .expect("beta's strip");
    egui::pos2(at.x - 6.0 - octa::ui::tiles::GAP / 2.0, 300.0)
}

#[test]
fn dragging_the_line_resizes_the_tiles_and_a_double_click_evens_them() {
    let ctx = egui::Context::default();
    let mut app = tiled_app();
    frame(&ctx, &mut app, vec![]);
    let line = line_left_of_beta(&ctx, &mut app);
    let to = line + egui::vec2(100.0, 0.0);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(line)]);
    frame(&ctx, &mut app, vec![button(line, true)]);
    for step in [line + egui::vec2(50.0, 0.0), to] {
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(step)]);
    }
    frame(&ctx, &mut app, vec![button(to, false)]);
    let xs = app.tile_sets[0].xs.clone();
    assert!(xs.len() == 1 && xs[0] > 0.55, "alpha grew: {xs:?}");
    assert_eq!(app.active_tab, 0, "the line belongs to no tile");

    let line = line_left_of_beta(&ctx, &mut app);
    assert!((line.x - to.x).abs() < 2.0, "the line followed the pointer");
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(line)]);
    for pressed in [true, false, true, false] {
        frame(&ctx, &mut app, vec![button(line, pressed)]);
    }
    assert!(app.tile_sets[0].xs.is_empty(), "even again");
}
