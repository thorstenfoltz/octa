//! Window section: startup size and position behaviour.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn window_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_window")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.start_maximised"))
                    .on_hover_text(crate::i18n::t("settings_hint.start_maximised"));
                ui.checkbox(&mut self.draft.start_maximized, "")
                    .on_hover_text(crate::i18n::t("settings_hint.start_maximised"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.initial_window_size"))
                    .on_hover_text(crate::i18n::t("settings_hint.initial_window_size"));
                ui.add_enabled_ui(!self.draft.start_maximized, |ui| {
                    egui::ComboBox::from_id_salt("window_size_combo")
                        .selected_text(self.draft.window_size.label())
                        .show_ui(ui, |ui| {
                            for &size in WindowSize::ALL {
                                ui.selectable_value(
                                    &mut self.draft.window_size,
                                    size,
                                    size.label(),
                                );
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("settings_hint.initial_window_size"));
                });
                ui.end_row();
            });
    }
}
