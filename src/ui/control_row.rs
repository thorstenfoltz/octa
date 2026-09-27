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
        egui::Grid::new(id)
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
