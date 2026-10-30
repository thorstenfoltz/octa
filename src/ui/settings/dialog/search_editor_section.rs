//! Search & text views section: default search mode, history size, tab size,
//! the click that adds a caret, the Raw view's column colours and its
//! warning, and where a notebook's output goes.
//!
//! The last three came from the old File-Specific section on 2026-10-30,
//! rows unchanged. `draw_sections` keeps the `CollapsingHeader` and calls
//! this.

use egui;

use super::super::*;
use crate::data::{SearchMode, SearchResultMode};

impl SettingsDialog {
    pub(super) fn search_editor_section_body(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::wrapping_grid(ui, "settings_search_editor")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                crate::ui::control_row::pin_control_height(ui);
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

                ui.label(crate::i18n::t("settings.caret_click"))
                    .on_hover_text(crate::i18n::t("settings_hint.caret_click"));
                egui::ComboBox::from_id_salt("caret_click_combo")
                    .selected_text(crate::i18n::t(self.draft.caret_click.label_key()))
                    .show_ui(ui, |ui| {
                        for c in CaretClick::ALL {
                            ui.selectable_value(
                                &mut self.draft.caret_click,
                                c,
                                crate::i18n::t(c.label_key()),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.caret_click"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.color_aligned"))
                    .on_hover_text(crate::i18n::t("settings_hint.color_aligned"));
                ui.checkbox(&mut self.draft.color_aligned_columns, "")
                    .on_hover_text(crate::i18n::t("settings_hint.color_aligned"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.warn_unalign"))
                    .on_hover_text(crate::i18n::t("settings_hint.warn_unalign"));
                ui.checkbox(&mut self.draft.warn_raw_align_reload, "")
                    .on_hover_text(crate::i18n::t("settings_hint.warn_unalign"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.notebook_output"))
                    .on_hover_text(crate::i18n::t("settings_hint.notebook_output"));
                egui::ComboBox::from_id_salt("notebook_layout_combo")
                    .selected_text(self.draft.notebook_output_layout.label_t())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.draft.notebook_output_layout,
                            NotebookOutputLayout::Beside,
                            crate::i18n::t("enum.nb_beside"),
                        );
                        ui.selectable_value(
                            &mut self.draft.notebook_output_layout,
                            NotebookOutputLayout::Beneath,
                            crate::i18n::t("enum.nb_beneath"),
                        );
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.notebook_output"));
                ui.end_row();
            });
    }
}
