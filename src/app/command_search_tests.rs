use super::*;

/// A text field elsewhere in the window, like the SQL editor.
fn other_field() -> egui::Id {
    egui::Id::new("a_field_elsewhere")
}

fn pointer(at: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn key(key: egui::Key) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

/// One frame in the app's own order: fire what was queued, read the shortcuts,
/// draw what lies under the box, then the box.
fn step(app: &mut OctaApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let mut text = String::new();
    let mut out = ctx.run_ui(input, |ui| {
        octa::ui::shortcuts::begin_frame();
        app.handle_shortcuts(ui.ctx());
        ui.add(egui::TextEdit::singleline(&mut text).id(other_field()));
        app.render_command_search(ui.ctx());
    });
    out.textures_delta.clear();
}

fn settle(app: &mut OctaApp, ctx: &egui::Context) {
    for _ in 0..3 {
        step(app, ctx, vec![]);
    }
}

/// Press, release, and the frame a queued command fires in.
fn click(app: &mut OctaApp, ctx: &egui::Context, at: egui::Pos2) {
    step(app, ctx, pointer(at, true));
    step(app, ctx, pointer(at, false));
    settle(app, ctx);
}

/// The box open with `query` typed and laid out.
fn opened(query: &str) -> (OctaApp, egui::Context) {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let ctx = egui::Context::default();
    app.open_command_search(&ctx);
    app.command_search.as_mut().unwrap().query = query.into();
    settle(&mut app, &ctx);
    (app, ctx)
}

fn window_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.memory(|m| m.area_rect(window_id()))
        .expect("the box was drawn")
}

const OUTSIDE: egui::Pos2 = egui::pos2(950.0, 750.0);

/// Every text egui paints in a frame, with where it starts.
fn painted_texts(app: &mut OctaApp, ctx: &egui::Context) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.pos)),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.render_command_search(ui.ctx());
    });
    output.textures_delta.clear();
    let mut texts = Vec::new();
    output
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, &mut texts));
    texts
}

/// A row now fills the width, and a centred label would read badly in a list.
#[test]
fn it_shows_its_title_and_row_text_starts_at_the_left() {
    let (mut app, ctx) = opened("new table");
    let window = window_rect(&ctx);
    let texts = painted_texts(&mut app, &ctx);

    assert!(
        texts.iter().any(|(t, _)| t == "Find a command"),
        "no title in {texts:?}"
    );
    let (_, row) = texts
        .iter()
        .find(|(t, _)| t.starts_with("New table"))
        .expect("the row's text");
    assert!(
        row.x < window.min.x + 40.0,
        "row text at {row:?} in {window:?}"
    );
}

#[test]
fn the_click_that_opens_it_from_the_help_menu_does_not_close_it() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let ctx = egui::Context::default();
    step(&mut app, &ctx, pointer(OUTSIDE, true));
    app.open_command_search(&ctx);
    step(&mut app, &ctx, pointer(OUTSIDE, false));
    settle(&mut app, &ctx);
    assert!(app.command_search.is_some());
}

/// The box has a close button and can be minimised, so like any other dialog it
/// stays until it is closed.
#[test]
fn a_click_outside_leaves_it_open() {
    let (mut app, ctx) = opened("");
    click(&mut app, &ctx, OUTSIDE);
    assert!(app.command_search.is_some());
}

/// The user's report: rows were only as wide as their words, so a click beside
/// the words hit nothing at all.
#[test]
fn a_click_anywhere_on_a_row_runs_it() {
    let (_, ctx) = opened("new table");
    let window = window_rect(&ctx);
    let ran = (0..160).step_by(8).any(|dy| {
        let (mut app, ctx) = opened("new table");
        // Far to the right of the row's few words.
        click(
            &mut app,
            &ctx,
            egui::pos2(window.max.x - 40.0, window.min.y + dy as f32),
        );
        app.new_table_dialog.is_some()
    });
    assert!(ran);
}

/// With the SQL editor or the search box focused, focus went back to it and
/// `text_edit_focused` then swallowed every command opening a dialog.
#[test]
fn a_picked_command_runs_though_a_text_field_had_focus() {
    let (mut app, ctx) = opened("");
    app.command_search = None;
    step(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(other_field()));
    step(&mut app, &ctx, vec![]);

    app.open_command_search(&ctx);
    app.command_search.as_mut().unwrap().query = "open url".into();
    settle(&mut app, &ctx);
    step(&mut app, &ctx, key(egui::Key::Enter));
    settle(&mut app, &ctx);

    assert!(app.command_search.is_none());
    assert!(app.open_url_dialog.is_some());
    assert!(
        ctx.memory(|m| m.has_focus(other_field())),
        "typing can go on where it was"
    );
}

#[test]
fn escape_gives_focus_back_to_the_field_it_came_from() {
    let (mut app, ctx) = opened("");
    app.command_search = None;
    step(&mut app, &ctx, vec![]);
    ctx.memory_mut(|m| m.request_focus(other_field()));
    step(&mut app, &ctx, vec![]);

    app.open_command_search(&ctx);
    settle(&mut app, &ctx);
    assert!(ctx.memory(|m| m.has_focus(field_id())));
    step(&mut app, &ctx, key(egui::Key::Escape));
    settle(&mut app, &ctx);

    assert!(app.command_search.is_none());
    assert!(ctx.memory(|m| m.has_focus(other_field())));
}

#[test]
fn its_title_bar_closes_minimises_and_maximises() {
    let (_, ctx) = opened("");
    let normal = window_rect(&ctx);
    let (mut closed, mut minimised, mut maximised) = (false, false, false);
    // Along the title bar, from the top right corner where the buttons sit.
    for dx in (8..130).step_by(8) {
        let (mut app, ctx) = opened("");
        let at = egui::pos2(normal.max.x - dx as f32, normal.min.y + 27.0);
        click(&mut app, &ctx, at);
        if app.command_search.is_none() {
            closed = true;
        } else {
            let rect = window_rect(&ctx);
            minimised |= rect.height() < normal.height() * 0.5;
            maximised |= rect.width() > normal.width() + 100.0;
        }
    }
    assert!(closed, "no close button");
    assert!(minimised, "no minimise button");
    assert!(maximised, "no maximise button");
}

#[test]
fn it_can_be_resized_by_dragging_its_corner() {
    let (mut app, ctx) = opened("");
    let before = window_rect(&ctx);
    let corner = before.max - egui::vec2(2.0, 2.0);
    let moved = corner + egui::vec2(120.0, 90.0);

    step(&mut app, &ctx, vec![egui::Event::PointerMoved(corner)]);
    step(&mut app, &ctx, pointer(corner, true));
    for t in 1..=6 {
        let at = corner + (moved - corner) * (t as f32 / 6.0);
        step(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
    }
    step(&mut app, &ctx, pointer(moved, false));
    settle(&mut app, &ctx);

    let after = window_rect(&ctx);
    assert!(
        after.width() > before.width() + 60.0,
        "{before:?} {after:?}"
    );
    assert!(
        after.height() > before.height() + 40.0,
        "{before:?} {after:?}"
    );
}

#[test]
fn the_keyboard_drives_the_box_while_its_field_has_focus() {
    let (mut app, ctx) = opened("");
    step(&mut app, &ctx, key(egui::Key::ArrowDown));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 1);
    step(&mut app, &ctx, key(egui::Key::ArrowUp));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 0);
    step(&mut app, &ctx, key(egui::Key::Escape));
    assert!(app.command_search.is_none());
}

/// It can stay open now, so it must not eat the table's arrow keys meanwhile.
#[test]
fn keys_stay_with_the_app_once_the_box_lost_focus() {
    let (mut app, ctx) = opened("");
    click(&mut app, &ctx, OUTSIDE);
    assert!(!ctx.memory(|m| m.has_focus(field_id())));

    step(&mut app, &ctx, key(egui::Key::ArrowDown));
    assert_eq!(app.command_search.as_ref().unwrap().selected, 0);
    step(&mut app, &ctx, key(egui::Key::Enter));
    assert!(app.command_search.is_some(), "Enter ran a command");
    step(&mut app, &ctx, key(egui::Key::Escape));
    assert!(app.command_search.is_some(), "Escape closed it");
}

#[test]
fn opening_it_again_brings_it_back_with_what_was_typed() {
    let (mut app, ctx) = opened("new");
    ctx.data_mut(|d| d.insert_temp(size_key(), DialogSize::Minimized));
    click(&mut app, &ctx, OUTSIDE);
    assert!(!ctx.memory(|m| m.has_focus(field_id())));

    app.open_command_search(&ctx);
    settle(&mut app, &ctx);

    assert_eq!(app.command_search.as_ref().unwrap().query, "new");
    assert!(ctx.memory(|m| m.has_focus(field_id())));
    assert!(matches!(
        ctx.data(|d| d.get_temp::<DialogSize>(size_key())),
        Some(DialogSize::Normal)
    ));
}

/// A feature inside a dialog is found by its own name, in the active language
/// and in English, and says which command opens it.
#[test]
fn a_feature_inside_a_dialog_is_listed_under_its_own_name() {
    let (app, _) = opened("");
    for (query, action) in [
        ("seasonality", SA::OpenChart),
        ("pareto", SA::OpenChart),
        ("working days", SA::OpenTimeCalc),
        ("word pairs", SA::ColumnValueFrequency),
        ("neighbours", SA::OpenOutliers),
        ("abc class", SA::OpenCalculatedColumn),
        ("visit number", SA::OpenCalculatedColumn),
        ("previous row", SA::OpenCalculatedColumn),
        ("currency", SA::OpenReferenceData),
        ("inflation", SA::OpenReferenceData),
        ("postcode", SA::OpenReferenceData),
        ("NUTS", SA::OpenReferenceData),
        ("Kreisschlüssel", SA::OpenReferenceData),
    ] {
        let rows = app.command_rows(query);
        let row = rows
            .iter()
            .find(|r| r.action == action)
            .unwrap_or_else(|| panic!("{query} finds nothing"));
        assert_eq!(row.place, action.display_name(), "{query}");
    }
}

/// A typo in `INSIDE` would list the raw key as a command name.
#[test]
fn every_inside_name_is_a_real_key() {
    for &(key, _) in INSIDE {
        assert!(!t(key).contains('.'), "{key} does not resolve");
    }
}
