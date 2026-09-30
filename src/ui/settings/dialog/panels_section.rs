//! Panels section: where the dockable side panels dock.
//!
//! Split off from Table View, which had collected them because they were the
//! newest settings rather than because they belong there. A panel position
//! says nothing about how the grid renders, and the section exists so the
//! next dockable panel has somewhere obvious to go.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn panels_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_panels")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.column_navigator_position"))
                    .on_hover_text(crate::i18n::t("settings_hint.column_navigator_position"));
                egui::ComboBox::from_id_salt("settings_column_navigator_position")
                    .selected_text(self.draft.column_navigator_position.label_t())
                    .show_ui(ui, |ui| {
                        for &pos in PanelPosition::ALL {
                            ui.selectable_value(
                                &mut self.draft.column_navigator_position,
                                pos,
                                pos.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.column_navigator_position"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.edit_audit_position"))
                    .on_hover_text(crate::i18n::t("settings_hint.edit_audit_position"));
                egui::ComboBox::from_id_salt("settings_edit_audit_position")
                    .selected_text(self.draft.edit_audit_position.label_t())
                    .show_ui(ui, |ui| {
                        for &pos in PanelPosition::ALL {
                            ui.selectable_value(
                                &mut self.draft.edit_audit_position,
                                pos,
                                pos.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.edit_audit_position"));
                ui.end_row();

                // The two connection browsers, each on its own edge if wanted.
                // Sharing an edge with each other or the folder browser stacks
                // them in one panel, as before.
                for (label, hint, id, value) in [
                    (
                        crate::i18n::t("settings.cloud_sidebar_position"),
                        crate::i18n::t("settings_hint.cloud_sidebar_position"),
                        "settings_cloud_sidebar_position",
                        &mut self.draft.cloud_sidebar_position,
                    ),
                    (
                        crate::i18n::t("settings.db_sidebar_position"),
                        crate::i18n::t("settings_hint.db_sidebar_position"),
                        "settings_db_sidebar_position",
                        &mut self.draft.db_sidebar_position,
                    ),
                ] {
                    ui.label(label).on_hover_text(&hint);
                    egui::ComboBox::from_id_salt(id)
                        .selected_text(value.label_t())
                        .show_ui(ui, |ui| {
                            for &pos in PanelPosition::ALL {
                                ui.selectable_value(&mut *value, pos, pos.label_t());
                            }
                        })
                        .response
                        .on_hover_text(&hint);
                    ui.end_row();
                }
            });
    }
}
