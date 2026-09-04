//! Keeping side panels from eating a small window.
//!
//! Every dockable panel (SQL, Assistant, multi-search, clean-up, the folder
//! sidebar) is an `egui::Panel` with a fixed `default_size` and `min_size`
//! chosen for a comfortable desktop window: 440px wide for a left/right panel,
//! 280px at its smallest. Those numbers are wider than the whole window once
//! it is dragged down near the 400x300 floor, and a panel whose minimum
//! exceeds the space available takes all of it, leaving the table with nothing
//! and the user with no way to get it back.
//!
//! [`clamp`] is the one place that decides how much of a window a panel may
//! claim. Panels ask for what they want; on a normal window they get exactly
//! that, and on a small one they get a share.

/// Largest fraction of the window one docked panel may take.
///
/// Two thirds: enough that a panel is still worth opening on a small window,
/// while the remaining third keeps the table visible rather than reduced to a
/// sliver. Panels stay resizable, so this bounds only what they *start* at and
/// how far the user can drag them, never what they may show.
const MAX_SHARE: f32 = 2.0 / 3.0;

/// Fit a panel's `(default_size, min_size)` to the space it actually has.
///
/// `available` is the extent the panel divides: the parent's width for a
/// left/right panel, its height for a top/bottom one. Returns the pair to hand
/// to `default_size()` and `min_size()`.
///
/// `min` is clamped first because it is the binding constraint - a minimum
/// larger than the window is what starves the table - and `default` is then
/// held at or above the clamped minimum so the two can never cross.
pub fn clamp(available: f32, default_size: f32, min_size: f32) -> (f32, f32) {
    // A window that reports no width yet (the first frame, or a minimised
    // window) must not collapse the panel to zero: pass the request through and
    // let the next frame, which has a real measurement, do the clamping.
    if !available.is_finite() || available <= 0.0 {
        return (default_size, min_size);
    }
    let ceiling = available * MAX_SHARE;
    let min = min_size.min(ceiling);
    let default = default_size.min(ceiling).max(min);
    (default, min)
}

#[cfg(test)]
#[path = "panel_fit_tests.rs"]
mod tests;
