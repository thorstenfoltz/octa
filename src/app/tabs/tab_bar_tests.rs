use super::*;

#[test]
fn the_drop_lands_where_the_pointer_passes_a_tab_centre() {
    let centres = [50.0, 150.0, 250.0, 350.0];
    assert_eq!(drop_index(&centres, 10.0, &[2]), 0);
    assert_eq!(drop_index(&centres, 200.0, &[0]), 1);
    assert_eq!(drop_index(&centres, 400.0, &[0]), 3);
    assert_eq!(drop_index(&centres, 260.0, &[2]), 2);
}

fn bar_app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for name in ["a", "b", "c"] {
        let mut t = TabState::new(app.settings.default_search_mode);
        t.raw_content = Some(name.to_string());
        t.table.source_path = Some(format!("/tmp/{name}.txt"));
        app.tabs.push(t);
    }
    app
}

/// One frame of the tab bar; returns every text it painted.
fn frame(ctx: &egui::Context, app: &mut OctaApp, events: Vec<egui::Event>) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 400.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_tab_bar(ui));
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

/// Press on tab 0, move right in steps to `to_x`, release; returns the app.
fn drag_first_tab_to(to_x: f32) -> OctaApp {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    frame(&ctx, &mut app, vec![]);
    let start = egui::pos2(20.0, 16.0);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(start)]);
    frame(&ctx, &mut app, vec![button(start, true)]);
    let mut x = start.x;
    while x < to_x {
        x = (x + 40.0).min(to_x);
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(x, 16.0))],
        );
    }
    frame(&ctx, &mut app, vec![button(egui::pos2(x, 16.0), false)]);
    frame(&ctx, &mut app, vec![]);
    app
}

fn names(app: &OctaApp) -> Vec<String> {
    app.tabs
        .iter()
        .map(|t| t.raw_content.clone().unwrap())
        .collect()
}

#[test]
fn dragging_a_tab_past_the_others_moves_it() {
    let app = drag_first_tab_to(800.0);
    assert_eq!(names(&app), ["b", "c", "a"]);
}

#[test]
fn a_click_without_movement_moves_nothing() {
    let app = drag_first_tab_to(20.0);
    assert_eq!(names(&app), ["a", "b", "c"]);
}

#[test]
fn a_folded_group_draws_its_chip_and_hides_its_tabs() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    frame(&ctx, &mut app, vec![]);
    let open = frame(&ctx, &mut app, vec![]);
    assert!(open.iter().any(|s| s == "G"), "{open:?}");
    assert!(open.iter().any(|s| s == "a.txt"), "{open:?}");

    app.toggle_fold(g);
    assert_eq!(app.active_tab, 2, "folding moves the active tab out");
    let folded = frame(&ctx, &mut app, vec![]);
    assert!(folded.iter().any(|s| s == "G"), "{folded:?}");
    assert!(folded.iter().any(|s| s == "c.txt"), "{folded:?}");
    assert!(
        !folded.iter().any(|s| s == "a.txt" || s == "b.txt"),
        "{folded:?}"
    );
}

#[test]
fn a_tab_dropped_between_two_members_joins_their_group() {
    // [a | b c] with b, c grouped: drag a past b's centre, before c's.
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[1, 2]);
    frame(&ctx, &mut app, vec![]);
    let centres = tab_centres(&ctx, &mut app);
    let mid = (centres[1] + centres[2]) / 2.0;
    drag(&ctx, &mut app, centres[0], mid);
    assert_eq!(names(&app), ["b", "a", "c"]);
    assert!(app.tabs.iter().all(|t| t.group == Some(g)));
}

#[test]
fn a_member_dropped_outside_leaves_its_group() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    frame(&ctx, &mut app, vec![]);
    let centres = tab_centres(&ctx, &mut app);
    drag(&ctx, &mut app, centres[0], centres[2] + 30.0);
    assert_eq!(names(&app), ["b", "c", "a"]);
    assert_eq!(app.tabs[0].group, Some(g));
    assert_eq!(app.tabs[2].group, None);
}

#[test]
fn dragging_the_chip_moves_the_whole_group() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    frame(&ctx, &mut app, vec![]);
    let chip = chip_centre(&ctx, &mut app);
    let centres = tab_centres(&ctx, &mut app);
    drag(&ctx, &mut app, chip, centres[2] + 30.0);
    assert_eq!(names(&app), ["c", "a", "b"]);
}

fn tab_rect(x: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(100.0, 28.0))
}

#[test]
fn only_the_middle_of_a_tab_takes_a_dropped_tab() {
    let rects = [tab_rect(0.0), tab_rect(100.0), tab_rect(200.0)];
    let at = |x, y| drop_onto(&[None, None, None], &rects, &[], 0, egui::pos2(x, y));
    assert_eq!(at(110.0, 10.0), None, "the left quarter reorders");
    assert_eq!(at(130.0, 10.0), Some(DropOnto::Tab(1)));
    assert_eq!(at(170.0, 10.0), Some(DropOnto::Tab(1)));
    assert_eq!(at(190.0, 10.0), None, "the right quarter reorders");
    assert_eq!(at(150.0, 60.0), None, "below the bar");
    assert_eq!(at(50.0, 10.0), None, "its own tab");
}

#[test]
fn a_group_takes_a_tab_only_from_outside() {
    let (g, h) = (GroupId(1), GroupId(2));
    let groups = [Some(g), Some(g), Some(h), None];
    let rects = [
        tab_rect(0.0),
        tab_rect(100.0),
        tab_rect(200.0),
        tab_rect(300.0),
    ];
    let chip = tab_rect(400.0);
    let chips = [(g, chip), (h, tab_rect(600.0))];
    let at = |from, x| drop_onto(&groups, &rects, &chips, from, egui::pos2(x, 10.0));

    assert_eq!(at(0, 150.0), None, "inside its own group a drag reorders");
    assert_eq!(at(0, 450.0), None, "so does its own chip");
    assert_eq!(at(0, 250.0), Some(DropOnto::Tab(2)), "another group");
    assert_eq!(at(0, 350.0), Some(DropOnto::Tab(3)), "a loose tab");
    assert_eq!(at(3, 450.0), Some(DropOnto::Chip(g)), "a chip");
    assert_eq!(at(3, 495.0), Some(DropOnto::Chip(g)), "to its right edge");
    assert_eq!(
        at(3, 410.0),
        None,
        "its left quarter lands before the group"
    );
}

#[test]
fn dragging_a_tab_onto_another_starts_a_group() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    frame(&ctx, &mut app, vec![]);
    let centres = tab_centres(&ctx, &mut app);
    drag(&ctx, &mut app, centres[0], centres[2]);
    assert_eq!(names(&app), ["b", "c", "a"]);
    let g = app.tabs[1].group.expect("c and a are grouped");
    assert_eq!(app.tabs[2].group, Some(g));
    assert_eq!(app.tabs[0].group, None);
    assert_eq!(app.tab_groups.len(), 1);
}

#[test]
fn dragging_a_tab_onto_a_member_joins_right_after_it() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[1, 2]);
    frame(&ctx, &mut app, vec![]);
    let centres = tab_centres(&ctx, &mut app);
    drag(&ctx, &mut app, centres[0], centres[1]);
    assert_eq!(names(&app), ["b", "a", "c"]);
    assert!(app.tabs.iter().all(|t| t.group == Some(g)));
    assert_eq!(app.tab_groups.len(), 1, "no second group");
}

#[test]
fn dragging_a_tab_onto_a_chip_joins_at_the_end() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[1, 2]);
    frame(&ctx, &mut app, vec![]);
    let (a, chip) = (tab_centres(&ctx, &mut app)[0], chip_centre(&ctx, &mut app));
    drag(&ctx, &mut app, a, chip);
    assert_eq!(names(&app), ["b", "c", "a"]);
    assert!(app.tabs.iter().all(|t| t.group == Some(g)));
}

#[test]
fn the_edge_of_a_tab_still_only_reorders() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    frame(&ctx, &mut app, vec![]);
    let centres = tab_centres(&ctx, &mut app);
    drag(&ctx, &mut app, centres[0], centres[2] + 30.0);
    assert_eq!(names(&app), ["b", "c", "a"]);
    assert!(app.tab_groups.is_empty());
}

/// The report: a group could be tiled from its chip, but not untiled there.
#[test]
fn the_chip_menu_untiles_a_tiled_group() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    app.tile_tabs(&[0, 1], TileLayout::Grid);
    frame(&ctx, &mut app, vec![]);

    let chip = text_centre(&ctx, &mut app, "G");
    click(&ctx, &mut app, chip, egui::PointerButton::Secondary);
    let untile = text_centre(&ctx, &mut app, &t("tiles.untile"));
    click(&ctx, &mut app, untile, egui::PointerButton::Primary);

    assert!(app.tabs.iter().all(|t| t.tile.is_none()));
    assert!(app.tile_sets.is_empty());
    assert_eq!(app.tabs[0].group, Some(g), "untiling keeps the group");
}

#[test]
fn the_whole_chip_folds_not_only_its_name() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    frame(&ctx, &mut app, vec![]);
    // Just past the name: where the arrow is painted.
    let name = text_centre(&ctx, &mut app, "G");
    let arrow = name + egui::vec2(12.0, 0.0);
    click(&ctx, &mut app, arrow, egui::PointerButton::Primary);
    assert!(app.tab_groups.iter().any(|x| x.id == g && x.folded));
}

/// Centre x of each tab label, read from where its text was painted.
fn tab_centres(ctx: &egui::Context, app: &mut OctaApp) -> Vec<f32> {
    let names: Vec<String> = app.tabs.iter().map(|t| t.title_display()).collect();
    names.iter().map(|n| text_x(ctx, app, n)).collect()
}

fn chip_centre(ctx: &egui::Context, app: &mut OctaApp) -> f32 {
    let name = app.tab_groups[0].name.clone();
    text_x(ctx, app, &name)
}

fn text_x(ctx: &egui::Context, app: &mut OctaApp, text: &str) -> f32 {
    text_centre(ctx, app, text).x
}

/// Move over `at`, press `button` and let go.
fn click(ctx: &egui::Context, app: &mut OctaApp, at: egui::Pos2, button: egui::PointerButton) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Default::default(),
    };
    frame(ctx, app, vec![egui::Event::PointerMoved(at)]);
    frame(ctx, app, vec![press(true)]);
    frame(ctx, app, vec![press(false)]);
    frame(ctx, app, vec![]);
}

fn text_centre(ctx: &egui::Context, app: &mut OctaApp, text: &str) -> egui::Pos2 {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 400.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_tab_bar(ui));
    out.textures_delta.clear();
    out.shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() / 2.0),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{text} not painted"))
}

/// Press at `from_x`, move in 20px steps to `to_x`, release.
fn drag(ctx: &egui::Context, app: &mut OctaApp, from_x: f32, to_x: f32) {
    let start = egui::pos2(from_x, 16.0);
    frame(ctx, app, vec![egui::Event::PointerMoved(start)]);
    frame(ctx, app, vec![button(start, true)]);
    let mut x = from_x;
    while (x - to_x).abs() > 0.1 {
        x = if to_x > x {
            (x + 20.0).min(to_x)
        } else {
            (x - 20.0).max(to_x)
        };
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(egui::pos2(x, 16.0))],
        );
    }
    frame(ctx, app, vec![button(egui::pos2(x, 16.0), false)]);
    frame(ctx, app, vec![]);
}

/// Right-click > Colour on a tab in no group: the tab fills with the colour,
/// and No colour takes it off again.
#[test]
fn the_tab_menu_colours_a_loose_tab_and_takes_it_off() {
    let ctx = egui::Context::default();
    let mut app = bar_app();
    let red = MarkColor::ALL[0];
    let tint = ThemeColors::mark_swatch(red).gamma_multiply(0.35);
    let painted = |app: &mut OctaApp| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, 400.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.render_tab_bar(ui));
        out.textures_delta.clear();
        out.shapes
            .iter()
            .any(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.fill == tint))
    };
    assert!(!painted(&mut app));

    let action = TabBarAction {
        set_colour: Some((1, Some(red))),
        ..Default::default()
    };
    app.apply_tab_bar_action(action, &ctx);
    assert_eq!(app.tabs[1].colour, Some(red));
    assert!(app.tabs[1].group.is_none(), "colouring makes no group");
    assert!(painted(&mut app), "tab b fills with its colour");

    let action = TabBarAction {
        set_colour: Some((1, None)),
        ..Default::default()
    };
    app.apply_tab_bar_action(action, &ctx);
    assert_eq!(app.tabs[1].colour, None);
    assert!(!painted(&mut app));
}

#[test]
fn next_tab_colour_walks_the_palette_and_ends_with_none() {
    use crate::app::tabs::groups::next_tab_colour;
    let mut seen = Vec::new();
    let mut c = next_tab_colour(None);
    while let Some(colour) = c {
        seen.push(colour);
        c = next_tab_colour(c);
    }
    assert_eq!(seen, MarkColor::ALL);
}
