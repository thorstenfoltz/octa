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

/// Which icon a [`caption_button`] paints.
#[derive(Clone, Copy)]
pub enum Caption {
    Close,
    Maximise,
    Minimise,
}

/// A title-bar button with a painted icon, shared by every dialog and the main
/// window's own title bar.
///
/// The icons used to be the text glyphs `x`, U+25A1 and `_`. They never lined
/// up: one is an x-height letter, one a symbol from a fallback font, one a
/// stroke on the baseline, and at their own font sizes they made the three
/// buttons different heights as well. Strokes painted around the button's
/// centre sit on one line whatever the font or theme.
pub fn caption_button(
    ui: &mut egui::Ui,
    caption: Caption,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    let response = ui.add(egui::Button::new("").selected(selected).min_size(size));
    let color = ui
        .style()
        .interact_selectable(&response, selected)
        .fg_stroke
        .color;
    let stroke = egui::Stroke::new(1.5, color);
    let c = response.rect.center();
    let r = 4.5;
    let painter = ui.painter();
    match caption {
        Caption::Close => {
            painter.line_segment([c + egui::vec2(-r, -r), c + egui::vec2(r, r)], stroke);
            painter.line_segment([c + egui::vec2(-r, r), c + egui::vec2(r, -r)], stroke);
        }
        Caption::Maximise => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::Vec2::splat(2.0 * r)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Caption::Minimise => {
            painter.line_segment([c + egui::vec2(-r, 0.0), c + egui::vec2(r, 0.0)], stroke);
        }
    }
    response
}

/// Render the three title-bar control buttons (Minimize, Maximize, Close)
/// into the current `ui` in right-to-left order (so the visual order is
/// `[_] [□] [x]`, matching desktop convention). Updates `*size` per click,
/// with mutual exclusion between Minimize and Maximize. Returns `true` when
/// the user clicked the close button.
///
/// The buttons are as tall as a themed button, so in the title row (a
/// [`crate::ui::control_row::control_line`]) they share the title's centre line.
pub fn draw_window_controls(ui: &mut egui::Ui, size: &mut DialogSize) -> bool {
    let btn_size = egui::vec2(26.0, crate::ui::control_row::control_height(ui));
    let close = caption_button(ui, Caption::Close, false, btn_size)
        .on_hover_text(crate::i18n::t("settings_hint.ctrl_close"))
        .clicked();
    let max_active = *size == DialogSize::Maximized;
    if caption_button(ui, Caption::Maximise, max_active, btn_size)
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
    let min_active = *size == DialogSize::Minimized;
    if caption_button(ui, Caption::Minimise, min_active, btn_size)
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

/// A dialog title row whose title carries a user's name (a file, a column, a
/// connection): the title on the left, [`draw_window_controls`] on the right.
/// Returns `true` when the user clicked the close button.
///
/// The usual title row draws the label first and the buttons in whatever is
/// left, which is fine for a fixed title but not for a name of any length: a
/// long one took the whole row and the buttons were painted over it. Here the
/// buttons are laid out first and the title gets only the width beside them,
/// cut short with an ellipsis (egui shows the whole of it on hover).
pub fn draw_title_bar(ui: &mut egui::Ui, title: egui::RichText, size: &mut DialogSize) -> bool {
    crate::ui::control_row::control_line(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = draw_window_controls(ui, size);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(title.strong().size(16.0)).truncate());
            });
            close
        })
        .inner
    })
    .inner
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

/// Centre a window the first time it is shown, and only then.
///
/// Use this instead of `.anchor(Align2::CENTER_CENTER, ..)`. An anchored egui
/// window is **immovable**: `Area::anchor` ends by calling `movable(false)`, so
/// the window ignores every drag and the user cannot push it aside to read what
/// is underneath it. `.default_pos` places it the same way on the first frame
/// and then gets out of the way, and egui remembers wherever the user drags it.
///
/// `size` is the window's expected size, used only to work out the centred
/// position; it does not constrain the window.
pub fn center_on_first_show(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
    ctx.viewport_rect().center() - size * 0.5
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
