//! Search & Editor section: default search mode, history size, tab size.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;
use crate::data::{SearchMode, SearchResultMode};

impl SettingsDialog {
    pub(super) fn search_editor_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_search_editor")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.default_search_mode"))
                    .on_hover_text(crate::i18n::t("settings_hint.default_search_mode"));
                egui::ComboBox::from_id_salt("search_mode_combo")
                    .selected_text(self.draft.default_search_mode.label_t())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.draft.default_search_mode,
                            SearchMode::Plain,
                            crate::i18n::t("enum.search_plain"),
                        );
                        ui.selectable_value(
                            &mut self.draft.default_search_mode,
                            SearchMode::Wildcard,
                            crate::i18n::t("enum.search_wildcard"),
                        );
                        ui.selectable_value(
                            &mut self.draft.default_search_mode,
                            SearchMode::Regex,
                            crate::i18n::t("enum.search_regex"),
                        );
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.default_search_mode"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.search_result_mode"))
                    .on_hover_text(crate::i18n::t("settings_hint.search_result_mode"));
                egui::ComboBox::from_id_salt("search_result_mode_combo")
                    .selected_text(self.draft.search_result_mode.label_t())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.draft.search_result_mode,
                            SearchResultMode::Filter,
                            crate::i18n::t("enum.search_result_filter"),
                        );
                        ui.selectable_value(
                            &mut self.draft.search_result_mode,
                            SearchResultMode::Highlight,
                            crate::i18n::t("enum.search_result_highlight"),
                        );
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.search_result_mode"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.search_history_limit"))
                    .on_hover_text(crate::i18n::t("settings_hint.search_history_limit"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_history_limit_buf)
                        .desired_width(120.0)
                        .hint_text("5"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.search_history_limit"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.tab_size"))
                    .on_hover_text(crate::i18n::t("settings_hint.tab_size"));
                egui::ComboBox::from_id_salt("tab_size_combo")
                    .selected_text(self.draft.tab_size.to_string())
                    .width(40.0)
                    .show_ui(ui, |ui| {
                        for n in 1..=16 {
                            ui.selectable_value(&mut self.draft.tab_size, n, n.to_string());
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.tab_size"));
                ui.end_row();
            });
    }
}
