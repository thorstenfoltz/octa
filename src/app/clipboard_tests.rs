use octa::ui::settings::{AppSettings, IconVariant};

use crate::app::state::OctaApp;

fn app(zoom_percent: u32) -> OctaApp {
    OctaApp::new(
        vec![],
        AppSettings {
            zoom_percent,
            ..AppSettings::default()
        },
        IconVariant::Rose,
    )
}

#[test]
fn octa_opens_at_the_zoom_it_was_left_at() {
    assert_eq!(app(150).zoom_percent, 150);
    assert_eq!(app(100).zoom_percent, 100);
    assert_eq!(app(9000).zoom_percent, 500, "a hand-edited settings.toml");
}

// `set_zoom` is `remember_zoom` plus a save, which no test may run: a save here
// writes the developer's real settings.toml. What it would write is checked below.
#[test]
fn a_new_zoom_is_what_the_next_start_reads() {
    let mut app = app(100);
    app.remember_zoom(130);
    assert_eq!(app.zoom_percent, 130);
    let written = toml::to_string(&app.settings).unwrap();
    let next: AppSettings = toml::from_str(&written).unwrap();
    assert_eq!(next.zoom_percent(), 130);

    app.remember_zoom(9000);
    assert_eq!(app.zoom_percent, 500);
    assert_eq!(app.settings.zoom_percent(), 500);
}
