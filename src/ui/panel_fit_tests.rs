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
