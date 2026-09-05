//! SQL section: panel position, row limit, autocomplete and editor font.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn sql_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_sql")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.sql_open_default"))
                    .on_hover_text(crate::i18n::t("settings_hint.sql_open_default"));
                ui.checkbox(&mut self.draft.sql_panel_default_open, "")
                    .on_hover_text(crate::i18n::t("settings_hint.sql_open_default"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.sql_panel_position"))
                    .on_hover_text(crate::i18n::t("settings_hint.sql_panel_position"));
                egui::ComboBox::from_id_salt("sql_panel_position_combo")
                    .selected_text(self.draft.sql_panel_position.label_t())
                    .show_ui(ui, |ui| {
                        for &pos in SqlPanelPosition::ALL {
                            ui.selectable_value(
                                &mut self.draft.sql_panel_position,
                                pos,
                                pos.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.sql_panel_position"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.default_row_limit"))
                    .on_hover_text(crate::i18n::t("settings_hint.sql_row_limit"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.sql_row_limit_buf)
                        .desired_width(80.0)
                        .hint_text("100"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.sql_row_limit"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.autocomplete"))
                    .on_hover_text(crate::i18n::t("settings_hint.autocomplete"));
                ui.checkbox(&mut self.draft.sql_autocomplete, "")
                    .on_hover_text(crate::i18n::t("settings_hint.autocomplete"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.editor_font"))
                    .on_hover_text(crate::i18n::t("settings_hint.editor_font"));
                egui::ComboBox::from_id_salt("sql_editor_font_combo")
                    .selected_text(self.draft.sql_editor_font.label_t())
                    .show_ui(ui, |ui| {
                        for &font in SqlEditorFont::ALL {
                            ui.selectable_value(
                                &mut self.draft.sql_editor_font,
                                font,
                                font.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.editor_font"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.sql_diff_highlight"))
                    .on_hover_text(crate::i18n::t("settings_hint.sql_diff_highlight"));
                ui.checkbox(&mut self.draft.sql_row_diff_highlight_enabled, "")
                    .on_hover_text(crate::i18n::t("settings_hint.sql_diff_highlight"));
                ui.end_row();

                ui.add_enabled_ui(self.draft.sql_row_diff_highlight_enabled, |ui| {
                    ui.label(crate::i18n::t("settings.sql_diff_secs"))
                        .on_hover_text(crate::i18n::t("settings_hint.sql_diff_secs"));
                });
                ui.add_enabled_ui(self.draft.sql_row_diff_highlight_enabled, |ui| {
                    egui::ComboBox::from_id_salt("sql_diff_secs_combo")
                        .selected_text(format!("{}", self.draft.sql_row_diff_highlight_secs))
                        .width(56.0)
                        .show_ui(ui, |ui| {
                            for n in [1u32, 2, 3, 4, 5, 8, 10, 15] {
                                ui.selectable_value(
                                    &mut self.draft.sql_row_diff_highlight_secs,
                                    n,
                                    n.to_string(),
                                );
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("settings_hint.sql_diff_secs"));
                });
                ui.end_row();
            });
    }
}
