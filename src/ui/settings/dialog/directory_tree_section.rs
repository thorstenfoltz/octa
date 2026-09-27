//! Directory Tree section: sidebar position and filtering.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn directory_tree_section_body(&mut self, ui: &mut egui::Ui) {
        Self::sub_section(
            ui,
            "settings.sub_browsing",
            "settings_dirtree_sub_browsing",
            |ui| {
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
            },
        );

        Self::sub_section(
            ui,
            "settings.sub_git_marks",
            "settings_dirtree_sub_git",
            |ui| {
                egui::Grid::new("settings_directory_tree_git")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(crate::i18n::t("settings.git_marks_uncommitted"))
                            .on_hover_text(crate::i18n::t("settings_hint.git_marks_uncommitted"));
                        ui.checkbox(&mut self.draft.git_marks_uncommitted, "")
                            .on_hover_text(crate::i18n::t("settings_hint.git_marks_uncommitted"));
                        ui.end_row();

                        ui.label(crate::i18n::t("settings.git_marks_branch"))
                            .on_hover_text(crate::i18n::t("settings_hint.git_marks_branch"));
                        ui.checkbox(&mut self.draft.git_marks_branch, "")
                            .on_hover_text(crate::i18n::t("settings_hint.git_marks_branch"));
                        ui.end_row();

                        let branch_on = self.draft.git_marks_branch;
                        let base_hint = if branch_on {
                            crate::i18n::t("settings_hint.git_marks_base_branch")
                        } else {
                            crate::i18n::t("settings_hint.git_marks_base_branch_off")
                        };
                        ui.add_enabled_ui(branch_on, |ui| {
                            ui.label(crate::i18n::t("settings.git_marks_base_branch"))
                                .on_hover_text(&base_hint)
                                .on_disabled_hover_text(&base_hint);
                        });
                        ui.add_enabled_ui(branch_on, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.draft.git_marks_base_branch)
                                    .desired_width(120.0)
                                    .hint_text("master"),
                            )
                            .on_hover_text(&base_hint)
                            .on_disabled_hover_text(&base_hint);
                        });
                        ui.end_row();

                        let any_on =
                            self.draft.git_marks_uncommitted || self.draft.git_marks_branch;
                        let secs_hint = if any_on {
                            crate::i18n::t("settings_hint.git_marks_refresh_secs")
                        } else {
                            crate::i18n::t("settings_hint.git_marks_refresh_secs_off")
                        };
                        ui.add_enabled_ui(any_on, |ui| {
                            ui.label(crate::i18n::t("settings.git_marks_refresh_secs"))
                                .on_hover_text(&secs_hint)
                                .on_disabled_hover_text(&secs_hint);
                        });
                        ui.add_enabled_ui(any_on, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.git_marks_refresh_secs_buf)
                                    .desired_width(60.0)
                                    .hint_text("10"),
                            )
                            .on_hover_text(&secs_hint)
                            .on_disabled_hover_text(&secs_hint);
                        });
                        ui.end_row();
                    });
            },
        );
    }
}
