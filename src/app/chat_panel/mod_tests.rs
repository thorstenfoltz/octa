use crate::app::state::OctaApp;

fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    octa::ui::settings::chat_profiles::ensure_profiles(&mut app.settings);
    app
}

/// How wide the panel's contents come out when the panel is `width` wide.
fn body_width(app: &mut OctaApp, width: f32) -> f32 {
    let ctx = egui::Context::default();
    let mut used = 0.0;
    // Twice: the inner panels size themselves from the frame before.
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(600.0, 0.0), egui::vec2(width, 800.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            app.render_chat_body(&mut child);
            used = child.min_rect().width();
        });
        out.textures_delta.clear();
    }
    used
}

/// A panel dragged narrower than its contents grows back to their width, and
/// egui clips what hangs over: the table loses the room and nothing is drawn
/// there (the user's "black field", 2026-11-10). So nothing in the panel may
/// be wider than the narrowest panel, 380px less the frame's margins.
#[test]
fn nothing_in_the_panel_is_wider_than_the_narrowest_panel() {
    let mut app = app();
    let used = body_width(&mut app, 360.0);
    assert!(used <= 360.5, "the contents take {used}px of 360");
}

/// One whole frame in the app's own order, returning where the panel's title
/// was painted.
fn title_x(app: &mut OctaApp) -> f32 {
    let ctx = egui::Context::default();
    let title = octa::i18n::t("chat.title");
    let mut x = f32::NAN;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            app.render_sql_panel(ui);
            app.render_chat_panel(ui);
            app.render_central_panel(ui);
        });
        out.textures_delta.clear();
        for clipped in &out.shapes {
            if let egui::Shape::Text(text) = &clipped.shape
                && text.galley.text() == title
            {
                x = text.pos.x;
            }
        }
    }
    x
}

#[test]
fn a_maximised_assistant_takes_the_tables_place_and_restore_docks_it_again() {
    let mut app = app();
    app.toggle_chat_panel();
    let docked = title_x(&mut app);
    assert!(docked > 600.0, "docked on the right, title at {docked}");

    app.toggle_chat_maximise();
    assert!(app.chat_maximised());
    let maximised = title_x(&mut app);
    assert!(
        maximised < 100.0,
        "it starts at the left edge, title at {maximised}"
    );

    app.toggle_chat_maximise();
    assert!((title_x(&mut app) - docked).abs() < 1.0, "back in its dock");
}

#[test]
fn closing_a_maximised_assistant_leaves_nothing_maximised_behind() {
    let mut app = app();
    // The shortcut on a closed panel opens it maximised.
    app.toggle_chat_maximise();
    assert!(app.chat.visible && app.chat_maximised());
    app.close_chat_panel();
    assert!(!app.chat_maximised());
    app.toggle_chat_panel();
    assert!(!app.chat_maximised(), "it opens docked the next time");
}

/// The Assistant has the window to itself: a docked SQL panel is not drawn
/// beside it, and a maximised one waits underneath.
#[test]
fn a_maximised_assistant_hides_the_sql_panel() {
    let mut app = app();
    app.open_sql_panel(0);
    app.toggle_chat_maximise();
    let x = title_x(&mut app);
    assert!(x < 100.0, "title at {x}");
    assert!(
        app.sql_panel_visible(),
        "the SQL panel is only covered, not closed"
    );
}
