//! Files section: recent-files count, open-as-text extensions, what Refresh
//! does, and auto-save.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn files_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_files")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
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

                ui.label(crate::i18n::t("settings.auto_save"))
                    .on_hover_text(crate::i18n::t("settings_hint.auto_save"));
                ui.checkbox(&mut self.draft.auto_save_enabled, "")
                    .on_hover_text(crate::i18n::t("settings_hint.auto_save"));
                ui.end_row();

                if self.draft.auto_save_enabled {
                    ui.label(crate::i18n::t("settings.auto_save_interval"))
                        .on_hover_text(crate::i18n::t("settings_hint.auto_save_interval"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.auto_save_interval_buf)
                            .desired_width(120.0)
                            .hint_text("5"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.auto_save_interval"));
                    ui.end_row();
                }

                ui.label(crate::i18n::t("settings.recipe_dir"))
                    .on_hover_text(crate::i18n::t("settings_hint.recipe_dir"));
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::i18n::t("settings.recipe_dir_choose"))
                        .on_hover_text(crate::i18n::t("settings_hint.recipe_dir"))
                        .clicked()
                        && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    {
                        self.draft.recipe_autosave_dir = Some(dir.display().to_string());
                    }
                    match &self.draft.recipe_autosave_dir {
                        Some(dir) => {
                            ui.label(egui::RichText::new(dir).weak());
                            if ui
                                .button(crate::i18n::t("settings.recipe_dir_reset"))
                                .on_hover_text(crate::i18n::t("settings_hint.recipe_dir_reset"))
                                .clicked()
                            {
                                self.draft.recipe_autosave_dir = None;
                            }
                        }
                        None => {
                            let path = crate::ui::settings::AppSettings::default_recipe_dir()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            ui.label(
                                egui::RichText::new(
                                    crate::i18n::t("settings.recipe_dir_default")
                                        .replace("{path}", &path),
                                )
                                .weak(),
                            );
                        }
                    }
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.recipe_autosave"))
                    .on_hover_text(crate::i18n::t("settings_hint.recipe_autosave"));
                ui.checkbox(&mut self.draft.recipe_autosave, "")
                    .on_hover_text(crate::i18n::t("settings_hint.recipe_autosave"));
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
            });

        // The write-option defaults are a group rather than a row pair, so
        // they get the shared expander instead of a grid line.
        crate::ui::settings::render_write_options(
            ui,
            &mut self.draft.write_options,
            &mut self.write_row_group_buf,
        );
    }
}
