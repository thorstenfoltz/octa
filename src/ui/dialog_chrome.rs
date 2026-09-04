//! Generic dialog chrome: the size mode every dialog window remembers, the
//! maximise/restore controls, and the shared result-message line.
//!
//! Moved out of `ui/settings/mod.rs`, where it had nothing to do with
//! settings: around seventy dialogs across the app use these. `ui::settings`
//! re-exports them, so every existing `settings::DialogSize` import still
//! resolves.
//!
//! Deliberately still four plain functions plus an enum. An earlier session
//! decided against inventing a shared chrome *abstraction*, and that decision
//! stands; this only gives the existing helpers an honest home.

use eframe::egui;

/// Window-size mode for a dialog. `Maximized` forces a full-screen rect;
/// `Minimized` hides the body so only the header bar is shown (the checkbox
/// stays visible there to restore). `Normal` is the default size.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum DialogSize {
    #[default]
    Normal,
    Maximized,
    Minimized,
}

/// Draw an operation's outcome line: green when it worked, the theme's error
/// colour when it did not, **wrapped** across as many lines as it needs.
///
/// Always call this on its own row, never inside a `ui.horizontal(..)`. An
/// egui horizontal layout gives its children infinite width, so a label in one
/// never wraps: a provider or driver error (which routinely runs to several
/// hundred characters) is then drawn as one line that disappears off the right
/// edge, and the part naming the actual problem is the part you cannot read.
pub fn draw_result_message(ui: &mut egui::Ui, ok: bool, msg: &str) {
    let color = if ok {
        egui::Color32::from_rgb(0x30, 0x80, 0x30)
    } else {
        ui.visuals().error_fg_color
    };
    ui.add(
        egui::Label::new(egui::RichText::new(msg).color(color))
            .wrap()
            // Selectable so a long error can be copied into a bug report or a
            // search box rather than retyped.
            .selectable(true),
    );
}

/// Render the three title-bar control buttons (Minimize, Maximize, Close)
/// into the current `ui` in right-to-left order (so the visual order is
/// `[_] [□] [x]`, matching desktop convention). Updates `*size` per click,
/// with mutual exclusion between Minimize and Maximize. Returns `true` when
/// the user clicked the close button.
///
/// Glyph choice: stick to characters the egui default font definitely
/// renders - underscore, U+25A1 white square, and `x`. Trying ─ / ⛶ / ✕
/// silently falls back to a missing-glyph box so all three buttons end up
/// visually identical.
pub fn draw_window_controls(ui: &mut egui::Ui, size: &mut DialogSize) -> bool {
    let btn_size = egui::vec2(26.0, 22.0);
    let mut close = false;

    // Close - bold lowercase `x`.
    if ui
        .add(egui::Button::new(egui::RichText::new("x").size(15.0).strong()).min_size(btn_size))
        .on_hover_text(crate::i18n::t("settings_hint.ctrl_close"))
        .clicked()
    {
        close = true;
    }
    // Maximize - U+25A1 WHITE SQUARE. `selected(active)` highlights it.
    let max_active = *size == DialogSize::Maximized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("\u{25A1}").size(14.0))
                .selected(max_active)
                .min_size(btn_size),
        )
        .on_hover_text(if max_active {
            crate::i18n::t("settings_hint.ctrl_restore")
        } else {
            crate::i18n::t("settings_hint.ctrl_full_size")
        })
        .clicked()
    {
        *size = if max_active {
            DialogSize::Normal
        } else {
            DialogSize::Maximized
        };
    }
    // Minimize - plain ASCII underscore, lowered visually so it sits where
    // the Windows minimize bar sits (the underscore baseline draws low,
    // matching the convention).
    let min_active = *size == DialogSize::Minimized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("_").size(15.0).strong())
                .selected(min_active)
                .min_size(btn_size),
        )
        .on_hover_text(if min_active {
            crate::i18n::t("settings_hint.ctrl_restore")
        } else {
            crate::i18n::t("settings_hint.ctrl_minimise")
        })
        .clicked()
    {
        *size = if min_active {
            DialogSize::Normal
        } else {
            DialogSize::Minimized
        };
    }
    close
}

/// Configure a dialog's `egui::Window` for the given [`DialogSize`], restoring
/// the pre-maximize size when the user un-maximizes.
///
/// egui persists a window's rect by id, so once a dialog is shown with
/// `fixed_rect(full_screen)` (Maximized) the stored rect stays full size and a
/// plain switch back to a resizable builder leaves the window stuck large -
/// the user "can't reduce it". This forces the remembered pre-maximize rect
/// for the single frame after un-maximizing, which overwrites egui's stored
/// rect; the window is resizable again from the next frame.
///
/// `id` must be stable per dialog and match the value passed to
/// [`remember_dialog_rect`]. `normal` applies the dialog's own Normal-size
/// builder settings (resizable, min/default size, default pos).
pub fn size_dialog_window<'a>(
    ctx: &egui::Context,
    id: egui::Id,
    size: DialogSize,
    window: egui::Window<'a>,
    normal: impl FnOnce(egui::Window<'a>) -> egui::Window<'a>,
) -> egui::Window<'a> {
    let prev_key = id.with("octa_dlg_prev_size");
    let rect_key = id.with("octa_dlg_normal_rect");
    // Track the previous frame's size so we can detect the Maximized -> Normal
    // transition that needs the one-frame restore.
    let prev = ctx.data_mut(|d| {
        let p = d.get_temp::<DialogSize>(prev_key);
        d.insert_temp(prev_key, size);
        p
    });
    // A dialog must never end up larger than the window it lives in. egui
    // clamps a `Resize`'s *default* size to the viewport but neither its
    // remembered `desired_size` nor its `min_size`, so a rect stored while the
    // window was large, or a `min_width` a dialog set for comfort, both survive
    // a shrink and leave half the dialog off-screen with its buttons
    // unreachable. In egui's resize maths `at_most(max)` is applied after
    // `at_least(min)`, so this one clamp wins for every dialog and none of them
    // need to know the window got small.
    //
    // Applied *before* `fixed_rect`, never after: `fixed_rect` sets min and max
    // to the same value, so a later `max_size` would fight it.
    let bounds = ctx.content_rect().shrink(8.0);
    match size {
        DialogSize::Maximized => window.fixed_rect(bounds),
        DialogSize::Minimized => window.max_size(bounds.size()).resizable(false),
        DialogSize::Normal => {
            if prev == Some(DialogSize::Maximized)
                && let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(rect_key))
            {
                normal(window).fixed_rect(clamp_rect(rect, bounds))
            } else {
                normal(window).max_size(bounds.size())
            }
        }
    }
}

/// Fit `rect` inside `bounds`: shrink it if it is too big, then slide it back
/// until it sits within the bounds. Keeps the top-left corner reachable, which
/// is the corner a dialog is dragged by.
fn clamp_rect(rect: egui::Rect, bounds: egui::Rect) -> egui::Rect {
    let size = rect.size().min(bounds.size());
    let x = rect
        .min
        .x
        .clamp(bounds.min.x, (bounds.max.x - size.x).max(bounds.min.x));
    let y = rect
        .min
        .y
        .clamp(bounds.min.y, (bounds.max.y - size.y).max(bounds.min.y));
    egui::Rect::from_min_size(egui::pos2(x, y), size)
}

/// Record a dialog window's current rect so a later un-maximize can restore it.
/// Call after `Window::show` with the inner response's rect. Only the Normal
/// mode's rect is remembered (the Maximized/Minimized rects are derived).
///
/// The passed `size` cannot be trusted: `draw_window_controls` flips it to
/// `Normal` on the click frame *while the window is still drawn maximized*, so
/// trusting it would store the full-screen rect as the restore position (the
/// "maximize, then can't go back" bug). Instead use the size the window was
/// actually built with this frame, which `size_dialog_window` records under the
/// shared `octa_dlg_prev_size` key.
pub fn remember_dialog_rect(ctx: &egui::Context, id: egui::Id, size: DialogSize, rect: egui::Rect) {
    let drawn = ctx
        .data(|d| d.get_temp::<DialogSize>(id.with("octa_dlg_prev_size")))
        .unwrap_or(size);
    if drawn == DialogSize::Normal {
        ctx.data_mut(|d| d.insert_temp(id.with("octa_dlg_normal_rect"), rect));
    }
}
