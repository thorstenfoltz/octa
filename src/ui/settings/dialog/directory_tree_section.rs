//! Directory Tree section: sidebar position and filtering.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn directory_tree_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_directory_tree")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.sidebar_position"))
                    .on_hover_text(crate::i18n::t("settings_hint.sidebar_position"));
                egui::ComboBox::from_id_salt("directory_tree_position_combo")
                    .selected_text(self.draft.directory_tree_position.label_t())
                    .show_ui(ui, |ui| {
                        for &pos in DirectoryTreePosition::ALL {
                            ui.selectable_value(
                                &mut self.draft.directory_tree_position,
                                pos,
                                pos.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.sidebar_position"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.directory_tree_filter"))
                    .on_hover_text(crate::i18n::t("settings_hint.directory_tree_filter"));
                ui.checkbox(&mut self.draft.directory_tree_filter_enabled, "")
                    .on_hover_text(crate::i18n::t("settings_hint.directory_tree_filter"));
                ui.end_row();
            });
    }
}
