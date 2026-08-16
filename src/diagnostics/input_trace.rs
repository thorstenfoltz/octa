//! Pointer / hit-test tracing for debug mode.
//!
//! Exists because of a bug that could not be reproduced on the developer's
//! machine: on some Linux desktops every widget *inside an `egui::Window`* went
//! dead to clicks while the same window still dragged and scrolled, and the
//! toolbar (a panel, so a different egui layer) kept working. Reading the code
//! could not distinguish the four ways egui can arrive at that state, so this
//! module makes the affected machine say which one it is.
//!
//! What it separates:
//!
//! - a foreground layer swallowing the pointer -> `layer` in the log names an
//!   area instead of the window,
//! - a widget whose interact rect is negative or NaN, so egui skips it and it
//!   is painted but inert -> egui's own `show_interactive_widgets` overlay
//!   paints no rect over it,
//! - a release egui refuses to count as a click (it rejects one when the
//!   pointer moved more than `max_click_dist` 6.0 points, or when press and
//!   release land in frames more than `max_click_duration` 0.8s apart) ->
//!   `press_origin` differs from `pos`, or the two lines are far apart in
//!   `time`,
//! - a scale-factor mismatch -> the startup line's two `ppp` values disagree.
//!
//! Everything is logged through `tracing`, so it lands in the rotating log at
//! `<config_dir>/logs/octa.log` that [`super::report`] already exports.
//!
//! The pointer log follows debug mode (the Settings checkbox or
//! `OCTA_DEBUG=1`). The visual overlays need `OCTA_DEBUG=1` specifically, since
//! they repaint the whole interface and nobody ticking "Debug logging" is
//! asking for that. Off, both are free.

use eframe::egui;

/// Trace pointer presses and releases, and switch on egui's own debug overlays.
///
/// Call once per frame from the update loop, only while debug mode is on.
/// Logs nothing on frames without a press or release, so a normal debug session
/// is not drowned by per-frame noise.
pub fn trace(ctx: &egui::Context) {
    setup_once(ctx);

    let (pressed, released) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.any_released()));
    if !pressed && !released {
        return;
    }

    let pos = ctx.input(|i| i.pointer.latest_pos());
    // The layer egui would route this pointer to. If a dialog's button is dead
    // because something else is on top, this is where it shows up.
    let layer = pos.and_then(|p| ctx.memory(|m| m.layer_id_at(p)));

    ctx.input(|i| {
        tracing::debug!(
            event = if pressed { "press" } else { "release" },
            ?pos,
            press_origin = ?i.pointer.press_origin(),
            time = i.time,
            any_click = i.pointer.any_click(),
            could_be_click = i.pointer.could_any_button_be_click(),
            decidedly_dragging = i.pointer.is_decidedly_dragging(),
            layer = ?layer,
            "pointer"
        );
    });
}

/// First frame only: log the numbers that stay the same all session but differ
/// between machines, and - only under `OCTA_DEBUG` - switch on egui's built-in
/// overlays.
///
/// The overlays are gated on the environment variable rather than on the
/// debug-mode setting, because the Settings checkbox is labelled "Debug
/// logging" and promises log detail. Painting a box around every widget in the
/// program is not what a user ticking that box asked for, so it stays on the
/// deliberate `OCTA_DEBUG=1 octa` path. `all_styles_mut` covers the light and
/// dark styles both, so the overlays survive a theme switch; it clones the
/// style, hence once rather than per frame.
fn setup_once(ctx: &egui::Context) {
    let key = egui::Id::new("octa_input_trace_logged");
    if ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, true));

    // `Style::debug` is `#[cfg(debug_assertions)]` in egui, so the overlays
    // exist only in a development build. A release binary run with OCTA_DEBUG
    // still gets the log, just no boxes - which is why the pointer trace above
    // carries the layer name in text: it has to stand on its own.
    #[cfg(debug_assertions)]
    if std::env::var_os("OCTA_DEBUG").is_some() {
        ctx.all_styles_mut(|s| {
            s.debug.debug_on_hover = true;
            s.debug.show_interactive_widgets = true;
            s.debug.show_widget_hits = true;
        });
    }

    tracing::debug!(
        ppp = ctx.pixels_per_point(),
        native_ppp = ?ctx.native_pixels_per_point(),
        zoom = ctx.zoom_factor(),
        viewport_rect = ?ctx.viewport_rect(),
        content_rect = ?ctx.content_rect(),
        session = ?std::env::var("XDG_SESSION_TYPE").ok(),
        desktop = ?std::env::var("XDG_CURRENT_DESKTOP").ok(),
        "input trace enabled"
    );
}
