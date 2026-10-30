//! Opening files section: everything that decides what happens when a file
//! opens. Recent files, extensions read as text, Refresh, the last session,
//! what is detected and cleaned on load, redirects, Excel sheets, the table
//! picker and the large-file notice.
//!
//! Gathered on 2026-10-30 from the old Files, File-Specific and Performance
//! sections, rows unchanged. `draw_sections` keeps the `CollapsingHeader` and
//! calls this.

use egui;

use super::super::*;
use super::ToggleWarning;

impl SettingsDialog {
    pub(super) fn opening_section_body(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::wrapping_grid(ui, "settings_opening")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                crate::ui::control_row::pin_control_height(ui);
                ui.label(crate::i18n::t("settings.max_recent"))
                    .on_hover_text(crate::i18n::t("settings_hint.max_recent"));
                egui::ComboBox::from_id_salt("max_recent_combo")
                    .selected_text(self.draft.max_recent_files.to_string())
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for n in 1..=30 {
                            ui.selectable_value(&mut self.draft.max_recent_files, n, n.to_string());
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.max_recent"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.open_as_text"))
                    .on_hover_text(crate::i18n::t("settings_hint.open_as_text"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.text_mode_extensions_buf)
                        .desired_width(280.0)
                        .hint_text("log4j, myproj, rawdata"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.open_as_text"));
                ui.end_row();

                ui.label(crate::i18n::t("refresh.setting"))
                    .on_hover_text(crate::i18n::t("refresh.setting_hint"));
                egui::ComboBox::from_id_salt("refresh_behaviour_combo")
                    .selected_text(crate::i18n::t(self.draft.refresh_behaviour.label_key()))
                    .show_ui(ui, |ui| {
                        for b in crate::ui::settings::RefreshBehaviour::ALL {
                            ui.selectable_value(
                                &mut self.draft.refresh_behaviour,
                                b,
                                crate::i18n::t(b.label_key()),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("refresh.setting_hint"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.restore_session"))
                    .on_hover_text(crate::i18n::t("settings_hint.restore_session"));
                ui.checkbox(&mut self.draft.restore_session, "")
                    .on_hover_text(crate::i18n::t("settings_hint.restore_session"));
                ui.end_row();

                // Cloud objects and connections are extra switches on top of
                // the main one, never implied by it: reopening them downloads
                // or queries at every start.
                let on = self.draft.restore_session;
                let off = crate::i18n::t("settings_hint.restore_session_needs_main");
                for (label, hint, value) in [
                    (
                        "settings.restore_session_cloud",
                        "settings_hint.restore_session_cloud",
                        &mut self.draft.restore_session_cloud,
                    ),
                    (
                        "settings.restore_session_connections",
                        "settings_hint.restore_session_connections",
                        &mut self.draft.restore_session_connections,
                    ),
                ] {
                    let hint = crate::i18n::t(hint);
                    ui.add_enabled_ui(on, |ui| {
                        ui.label(crate::i18n::t(label))
                            .on_hover_text(&hint)
                            .on_disabled_hover_text(&off);
                    });
                    ui.add_enabled_ui(on, |ui| {
                        ui.checkbox(value, "")
                            .on_hover_text(&hint)
                            .on_disabled_hover_text(&off);
                    });
                    ui.end_row();
                }

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

                ui.label(crate::i18n::t("settings.detect_times"))
                    .on_hover_text(crate::i18n::t("settings_hint.detect_times"));
                ui.checkbox(&mut self.draft.detect_times_on_open, "")
                    .on_hover_text(crate::i18n::t("settings_hint.detect_times"));
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

                ui.label(crate::i18n::t("settings.excel_auto_open"))
                    .on_hover_text(crate::i18n::t("settings_hint.excel_auto_open"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.excel_max_auto_sheets_buf)
                        .desired_width(120.0)
                        .hint_text("5"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.excel_auto_open"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.tables_in_picker"))
                    .on_hover_text(crate::i18n::t("settings_hint.tables_in_picker"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.table_picker_visible_rows_buf)
                        .desired_width(120.0)
                        .hint_text("10"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.tables_in_picker"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.show_large_file_notice"))
                    .on_hover_text(crate::i18n::t("settings_hint.show_large_file_notice"));
                ui.checkbox(&mut self.draft.show_large_file_notice, "")
                    .on_hover_text(crate::i18n::t("settings_hint.show_large_file_notice"));
                ui.end_row();
            });
    }
}
