//! Saving files section: auto-save, the Save As questions, recipes, earlier
//! versions and the write options.
//!
//! The saving half of the old Files section (the opening half went to
//! `opening_section.rs` on 2026-10-30), rows unchanged. `draw_sections` keeps
//! the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn saving_section_body(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::wrapping_grid(ui, "settings_saving")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                crate::ui::control_row::pin_control_height(ui);
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

                self.save_as_rows(ui);

                ui.label(crate::i18n::t("settings.recipe_dir"))
                    .on_hover_text(crate::i18n::t("settings_hint.recipe_dir"));
                crate::ui::control_row::control_row(ui, |ui| {
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

                self.versions_rows(ui);
            });

        // The write-option defaults are a group rather than a row pair, so
        // they get the shared expander instead of a grid line.
        crate::ui::settings::render_write_options(
            ui,
            &mut self.draft.write_options,
            &mut self.write_row_group_buf,
        );
    }

    /// The two Save As questions. Each is asked every time unless its box is
    /// unticked; the combo is then the answer used, and either way the answer
    /// the question starts at.
    fn save_as_rows(&mut self, ui: &mut egui::Ui) {
        ask_row(
            ui,
            (
                "settings.save_as_rows",
                "settings_hint.save_as_rows",
                "save_as_rows_combo",
            ),
            &mut self.draft.save_as_ask_rows,
            &mut self.draft.save_as_filtered_rows,
            [
                "settings.save_as_rows_all",
                "settings.save_as_rows_filtered",
            ],
        );
        ask_row(
            ui,
            (
                "settings.save_as_open",
                "settings_hint.save_as_open",
                "save_as_open_combo",
            ),
            &mut self.draft.save_as_ask_open,
            &mut self.draft.save_as_open_new_file,
            ["settings.save_as_open_no", "settings.save_as_open_yes"],
        );
    }

    /// Earlier versions: the switch, then the two limits, each a number with
    /// its own Unlimited box. Both unlimited is allowed and said out loud.
    fn versions_rows(&mut self, ui: &mut egui::Ui) {
        let t = crate::i18n::t;
        ui.label(t("settings.keep_versions"))
            .on_hover_text(t("settings_hint.keep_versions"));
        ui.checkbox(&mut self.draft.keep_versions, "")
            .on_hover_text(t("settings_hint.keep_versions"));
        ui.end_row();

        let on = self.draft.keep_versions;
        let needs_main = t("settings_hint.versions_needs_main");
        self.versions_place_row(ui, on, &needs_main);
        let limits: [(&str, &str, &mut String, &mut bool, Option<&str>); 2] = [
            (
                "settings.versions_per_file",
                "settings_hint.versions_per_file",
                &mut self.versions_per_file_buf,
                &mut self.draft.versions_per_file_unlimited,
                None,
            ),
            (
                "settings.versions_total",
                "settings_hint.versions_total",
                &mut self.versions_total_mb_buf,
                &mut self.draft.versions_total_unlimited,
                Some("MB"),
            ),
        ];
        for (label, hint, buf, unlimited, unit) in limits {
            let hint = t(hint);
            let field_off = if on {
                t("settings_hint.versions_limit_off")
            } else {
                needs_main.clone()
            };
            ui.add_enabled_ui(on, |ui| {
                ui.label(t(label))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&needs_main);
            });
            ui.add_enabled_ui(on, |ui| {
                crate::ui::control_row::control_line(ui, |ui| {
                    ui.add_enabled(
                        !*unlimited,
                        egui::TextEdit::singleline(buf).desired_width(80.0),
                    )
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&field_off);
                    if let Some(unit) = unit {
                        ui.label(unit);
                    }
                    ui.checkbox(unlimited, t("settings.unlimited"))
                        .on_hover_text(&hint)
                        .on_disabled_hover_text(&needs_main);
                });
            });
            ui.end_row();
        }
        if on && self.draft.versions_per_file_unlimited && self.draft.versions_total_unlimited {
            ui.label("");
            ui.label(
                egui::RichText::new(t("settings.versions_no_limit"))
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
            ui.end_row();
        }
    }

    /// Where the versions go: a combo, plus the folder picker once "A folder
    /// you choose" is picked.
    fn versions_place_row(&mut self, ui: &mut egui::Ui, on: bool, needs_main: &str) {
        use crate::ui::settings::VersionsPlace;
        let t = crate::i18n::t;
        let hint = t("settings_hint.versions_place");
        ui.add_enabled_ui(on, |ui| {
            ui.label(t("settings.versions_place"))
                .on_hover_text(&hint)
                .on_disabled_hover_text(needs_main);
        });
        ui.add_enabled_ui(on, |ui| {
            crate::ui::control_row::control_line(ui, |ui| {
                egui::ComboBox::from_id_salt("versions_place_combo")
                    .selected_text(t(self.draft.versions_place.label_key()))
                    .show_ui(ui, |ui| {
                        for place in VersionsPlace::ALL {
                            ui.selectable_value(
                                &mut self.draft.versions_place,
                                place,
                                t(place.label_key()),
                            );
                        }
                    })
                    .response
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(needs_main);
                if self.draft.versions_place != VersionsPlace::Folder {
                    return;
                }
                let pick_hint = t("settings_hint.versions_dir_choose");
                if ui
                    .button(t("settings.versions_dir_choose"))
                    .on_hover_text(&pick_hint)
                    .on_disabled_hover_text(needs_main)
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.draft.versions_dir = Some(dir.display().to_string());
                }
                let shown = match &self.draft.versions_dir {
                    Some(dir) => dir.clone(),
                    None => t("settings.versions_dir_unset"),
                };
                ui.label(egui::RichText::new(shown).weak());
            });
        });
        ui.end_row();
    }
}

/// One Save As question row: its label, an **Ask every time** box and a combo
/// with the answer (`answers[false]`, `answers[true]`). `keys` are the label
/// key, the hint key and the combo's id.
fn ask_row(
    ui: &mut egui::Ui,
    (label, hint, combo_id): (&str, &str, &str),
    ask: &mut bool,
    answer: &mut bool,
    answers: [&str; 2],
) {
    let t = crate::i18n::t;
    let hint = t(hint);
    ui.label(t(label)).on_hover_text(&hint);
    crate::ui::control_row::control_line(ui, |ui| {
        ui.checkbox(ask, t("settings.save_as_ask"))
            .on_hover_text(&hint);
        egui::ComboBox::from_id_salt(combo_id)
            .selected_text(t(answers[usize::from(*answer)]))
            .show_ui(ui, |ui| {
                for value in [false, true] {
                    ui.selectable_value(answer, value, t(answers[usize::from(value)]));
                }
            })
            .response
            .on_hover_text(&hint);
    });
    ui.end_row();
}
