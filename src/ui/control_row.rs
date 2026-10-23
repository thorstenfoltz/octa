//! Rows of controls that line up.
//!
//! A row mixing a label, a `ComboBox`, a button and a checkbox comes out
//! ragged by default, and this has been reported in feature after feature
//! (SQL ask row, chart bar, timeline pickers, join dialog). The cause is
//! always the same: a button is `font + 2 * button_padding.y` tall (Octa's
//! themes pad 5-7px), while a `ComboBox` lays its button out in a nested
//! `horizontal` whose band starts at `interact_size.y` (egui's 18px) and is
//! then centred in the taller row, so it sinks a few pixels. Egui's default
//! style pads buttons by 1px, which hides the defect in a toy reproduction.
//!
//! Every row or grid of form controls goes through [`control_row`] or
//! [`control_grid`], which pin `interact_size.y` to the button height so
//! every widget shares one centre line.

use eframe::egui;

/// The height of a plain button in the current style; every control in a
/// [`control_row`] or [`control_grid`] is at least this tall.
pub fn control_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y
}

/// A wrapping row of controls sharing one centre line.
pub fn control_row<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let h = control_height(ui);
    ui.horizontal_wrapped(|ui| {
        // `interact_size` is the *minimum* size of an interactive widget, which
        // is what ComboBox and checkbox size themselves against.
        ui.spacing_mut().interact_size.y = h;
        ui.set_min_height(h);
        body(ui)
    })
    .inner
}

/// [`control_row`] without the wrapping: the drop-in for a `ui.horizontal`
/// holding controls, with the same return type. A bare `ui.horizontal` starts
/// its row `interact_size.y` (18px) tall, so a label or checkbox placed before
/// the first themed button centres 4-5px above it, and a ComboBox placed after
/// one sinks by as much. Dialog title bars had the same fault: the title sat
/// above the minimise / maximise / close buttons.
pub fn control_line<R>(
    ui: &mut egui::Ui,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let h = control_height(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = h;
        ui.set_min_height(h);
        body(ui)
    })
}

/// Pin a grid of controls to one height. Call it first in the `show` closure
/// of an `egui::Grid` holding a ComboBox ([`control_grid`] already does it for
/// label / control pairs): a ComboBox wraps itself in a `ui.horizontal` whose
/// band is `interact_size.y`, so at egui's 18px it sits 4.5px below the themed
/// buttons and text fields in its row.
pub fn pin_control_height(ui: &mut egui::Ui) {
    ui.spacing_mut().interact_size.y = control_height(ui);
}

/// An `egui::Grid` whose last column wraps its text at the right edge.
///
/// A grid lays text out on one endless line unless it has a maximum column
/// width, so a long note or checkbox label in the last column ran past the
/// window and was cut off, however wide the window was dragged (the SFTP
/// agent note in Settings). Capping at the full width wraps the last column
/// at the edge and leaves the earlier ones as wide as their widest cell.
pub fn wrapping_grid(ui: &egui::Ui, id: impl egui::AsIdSalt) -> egui::Grid {
    egui::Grid::new(id).max_col_width(ui.available_width())
}

/// A list grid: per row a text, then one button per `buttons` label, in that
/// order. The text column wraps short of the right edge, leaving room for
/// the buttons.
///
/// [`wrapping_grid`] wraps only its last column, so a long first column
/// (a connection's name in Settings) ran on to the window's width and pushed
/// its Edit and Remove buttons out of view.
pub fn button_list_grid(ui: &egui::Ui, id: impl egui::AsIdSalt, buttons: &[&str]) -> egui::Grid {
    const GAP: f32 = 8.0;
    let pad = 2.0 * ui.spacing().button_padding.x;
    let buttons_width: f32 = buttons
        .iter()
        .map(|label| {
            let text = egui::WidgetText::from(*label).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            );
            // + 2: a frame's stroke.
            GAP + text.size().x + pad + 2.0
        })
        .sum();
    egui::Grid::new(id)
        .num_columns(buttons.len() + 1)
        .spacing([GAP, 4.0])
        .max_col_width((ui.available_width() - buttons_width).max(120.0))
}

/// A label / control grid (one `ui.end_row()` per line) whose columns line
/// up and whose rows share one centre line.
pub fn control_grid<R>(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let h = control_height(ui);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = h;
        wrapping_grid(ui, id)
            .num_columns(2)
            .min_row_height(h)
            .spacing([8.0, 6.0])
            .show(ui, body)
            .inner
    })
    .inner
}

/// A text field of the row's height. `TextEdit` sizes itself from its font
/// and margin, not from `interact_size`, so the height is imposed.
pub fn control_text_edit(
    ui: &mut egui::Ui,
    width: f32,
    edit: egui::TextEdit<'_>,
) -> egui::Response {
    let h = control_height(ui);
    ui.add_sized([width, h], edit)
}

#[cfg(test)]
#[path = "control_row_tests.rs"]
mod tests;
