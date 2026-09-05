//! Table View section: row numbers, colours, number display and cell rendering.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;
use crate::data::BinaryDisplayMode;
use crate::data::MarkColor;

impl SettingsDialog {
    pub(super) fn table_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_table")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.show_row_numbers"))
                    .on_hover_text(crate::i18n::t("settings_hint.show_row_numbers"));
                ui.checkbox(&mut self.draft.show_row_numbers, "")
                    .on_hover_text(crate::i18n::t("settings_hint.show_row_numbers"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.show_sequential_rows"))
                    .on_hover_text(crate::i18n::t("settings_hint.show_sequential_rows"));
                ui.checkbox(&mut self.draft.show_sequential_row_numbers, "")
                    .on_hover_text(crate::i18n::t("settings_hint.show_sequential_rows"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.alternating_rows"))
                    .on_hover_text(crate::i18n::t("settings_hint.alternating_rows"));
                ui.checkbox(&mut self.draft.alternating_row_colors, "")
                    .on_hover_text(crate::i18n::t("settings_hint.alternating_rows"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.negative_red"))
                    .on_hover_text(crate::i18n::t("settings_hint.negative_red"));
                ui.checkbox(&mut self.draft.negative_numbers_red, "")
                    .on_hover_text(crate::i18n::t("settings_hint.negative_red"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.mark_filter_cell_mode"))
                    .on_hover_text(crate::i18n::t("settings_hint.mark_filter_cell_mode"));
                egui::ComboBox::from_id_salt("settings_mark_filter_cell_mode")
                    .selected_text(crate::i18n::t(self.draft.mark_filter_cell_mode.i18n_key()))
                    .show_ui(ui, |ui| {
                        for mode in crate::data::mark_filter::MarkFilterCellMode::ALL
                            .iter()
                            .copied()
                        {
                            ui.selectable_value(
                                &mut self.draft.mark_filter_cell_mode,
                                mode,
                                crate::i18n::t(mode.i18n_key()),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.mark_filter_cell_mode"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.thousand_sep"))
                    .on_hover_text(crate::i18n::t("settings_hint.thousand_sep"));
                ui.checkbox(&mut self.draft.thousands_separators_in_cells, "")
                    .on_hover_text(crate::i18n::t("settings_hint.thousand_sep"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.number_style"))
                    .on_hover_text(crate::i18n::t("settings_hint.number_style"));
                egui::ComboBox::from_id_salt("settings_number_separator_style")
                    .selected_text(self.draft.number_separator_style.label_t())
                    .show_ui(ui, |ui| {
                        for style in crate::data::num_format::SeparatorStyle::ALL.iter().copied() {
                            ui.selectable_value(
                                &mut self.draft.number_separator_style,
                                style,
                                style.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.number_style"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.highlight_edits"))
                    .on_hover_text(crate::i18n::t("settings_hint.highlight_edits"));
                ui.checkbox(&mut self.draft.highlight_edits, "")
                    .on_hover_text(crate::i18n::t("settings_hint.highlight_edits"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.cell_line_breaks"))
                    .on_hover_text(crate::i18n::t("settings_hint.cell_line_breaks"));
                ui.checkbox(&mut self.draft.cell_line_breaks, "")
                    .on_hover_text(crate::i18n::t("settings_hint.cell_line_breaks"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.clickable_links"))
                    .on_hover_text(crate::i18n::t("settings_hint.clickable_links"));
                ui.checkbox(&mut self.draft.clickable_links, "")
                    .on_hover_text(crate::i18n::t("settings_hint.clickable_links"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.binary_display"))
                    .on_hover_text(crate::i18n::t("settings_hint.binary_display"));
                egui::ComboBox::from_id_salt("binary_display_combo")
                    .selected_text(self.draft.binary_display_mode.label_t())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.draft.binary_display_mode,
                            BinaryDisplayMode::Binary,
                            BinaryDisplayMode::Binary.label_t(),
                        );
                        ui.selectable_value(
                            &mut self.draft.binary_display_mode,
                            BinaryDisplayMode::Hex,
                            BinaryDisplayMode::Hex.label_t(),
                        );
                        ui.selectable_value(
                            &mut self.draft.binary_display_mode,
                            BinaryDisplayMode::Text,
                            BinaryDisplayMode::Text.label_t(),
                        );
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.binary_display"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.default_mark_color"))
                    .on_hover_text(crate::i18n::t("settings_hint.default_mark_color"));
                egui::ComboBox::from_id_salt("default_mark_color_combo")
                    .selected_text(self.draft.default_mark_color.label_t())
                    .show_ui(ui, |ui| {
                        for &color in MarkColor::ALL {
                            ui.selectable_value(
                                &mut self.draft.default_mark_color,
                                color,
                                color.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.default_mark_color"));
                ui.end_row();
            });
    }
}
