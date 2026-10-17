//! File-Specific section: per-format load and display options.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;
use super::ToggleWarning;

impl SettingsDialog {
    pub(super) fn format_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_format")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                crate::ui::control_row::pin_control_height(ui);
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

                ui.label(crate::i18n::t("settings.detect_dates"))
                    .on_hover_text(crate::i18n::t("settings_hint.detect_dates"));
                let dates_before = self.draft.detect_dates_on_load;
                ui.checkbox(&mut self.draft.detect_dates_on_load, "")
                    .on_hover_text(crate::i18n::t("settings_hint.detect_dates"));
                if !dates_before && self.draft.detect_dates_on_load {
                    self.toggle_warning = Some(ToggleWarning::DetectDatesOn);
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.warn_date_change"))
                    .on_hover_text(crate::i18n::t("settings_hint.warn_date_change"));
                ui.add_enabled(
                    self.draft.detect_dates_on_load,
                    egui::Checkbox::without_text(&mut self.draft.warn_on_date_format_change),
                )
                .on_hover_text(crate::i18n::t("settings_hint.warn_date_change"))
                .on_disabled_hover_text(crate::i18n::t("settings_hint.warn_date_change_off"));
                ui.end_row();

                // Belongs with the other "warn me before this happens"
                // toggles: it is about opening a file, not about cloud
                // connections.
                ui.label(crate::i18n::t("settings.confirm_url_redirects"))
                    .on_hover_text(crate::i18n::t("settings_hint.confirm_url_redirects"));
                let redirects_before = self.draft.confirm_url_redirects;
                ui.checkbox(&mut self.draft.confirm_url_redirects, "")
                    .on_hover_text(crate::i18n::t("settings_hint.confirm_url_redirects"));
                // Turning a safety check off deserves an explanation, not
                // a silent tick. Turning it back on does not.
                if redirects_before && !self.draft.confirm_url_redirects {
                    self.toggle_warning = Some(ToggleWarning::UrlRedirectsOff);
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.trim_whitespace"))
                    .on_hover_text(crate::i18n::t("settings_hint.trim_whitespace"));
                ui.checkbox(&mut self.draft.trim_whitespace_on_load, "")
                    .on_hover_text(crate::i18n::t("settings_hint.trim_whitespace"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.warn_trim"))
                    .on_hover_text(crate::i18n::t("settings_hint.warn_trim"));
                ui.checkbox(&mut self.draft.warn_on_whitespace_trim, "")
                    .on_hover_text(crate::i18n::t("settings_hint.warn_trim"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.clean_headers"))
                    .on_hover_text(crate::i18n::t("settings_hint.clean_headers"));
                ui.checkbox(&mut self.draft.clean_headers_on_load, "")
                    .on_hover_text(crate::i18n::t("settings_hint.clean_headers"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.offer_repair"))
                    .on_hover_text(crate::i18n::t("settings_hint.offer_repair"));
                ui.checkbox(&mut self.draft.offer_repair_on_malformed, "")
                    .on_hover_text(crate::i18n::t("settings_hint.offer_repair"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.readonly_notice"))
                    .on_hover_text(crate::i18n::t("settings_hint.readonly_notice"));
                ui.checkbox(&mut self.draft.show_readonly_notice, "")
                    .on_hover_text(crate::i18n::t("settings_hint.readonly_notice"));
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
