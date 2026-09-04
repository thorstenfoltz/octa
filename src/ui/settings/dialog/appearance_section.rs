//! Appearance section: language, fonts, theme, icon variant, window
//! controls and the status-message timeout.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;
use crate::ui::theme::{BodyFont, ThemeMode};

impl SettingsDialog {
    pub(super) fn appearance_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_appearance")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.language"))
                    .on_hover_text(crate::i18n::t("settings_hint.language"));
                let current_lang_label = crate::i18n::LANGUAGES
                    .iter()
                    .find(|(c, _)| *c == self.draft.language)
                    .map(|(_, name)| *name)
                    .unwrap_or("English");
                egui::ComboBox::from_id_salt("settings_language_combo")
                    .selected_text(current_lang_label)
                    .show_ui(ui, |ui| {
                        for (code, name) in crate::i18n::LANGUAGES {
                            ui.selectable_value(
                                &mut self.draft.language,
                                (*code).to_string(),
                                *name,
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.language"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.font_size"))
                    .on_hover_text(crate::i18n::t("settings_hint.font_size"));
                let old_size = self.draft.font_size;
                let current_pt = self.draft.font_size.round() as i32;
                egui::ComboBox::from_id_salt("font_size_combo")
                    .selected_text(format!("{} pt", current_pt))
                    .show_ui(ui, |ui| {
                        for sz in 8..=32 {
                            ui.selectable_value(
                                &mut self.draft.font_size,
                                sz as f32,
                                format!("{} pt", sz),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.font_size"));
                if self.draft.font_size != old_size {
                    self.font_changed = true;
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.default_theme"))
                    .on_hover_text(crate::i18n::t("settings_hint.default_theme"));
                let old_theme = self.draft.default_theme;
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(self.draft.default_theme.label())
                    .show_ui(ui, |ui| {
                        for &preset in ThemeMode::ALL {
                            ui.selectable_value(
                                &mut self.draft.default_theme,
                                preset,
                                preset.label(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.default_theme"));
                if self.draft.default_theme != old_theme {
                    self.theme_changed = true;
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.body_font"))
                    .on_hover_text(crate::i18n::t("settings_hint.body_font"));
                let old_body_font = self.draft.body_font;
                egui::ComboBox::from_id_salt("body_font_combo")
                    .selected_text(self.draft.body_font.label_t())
                    .show_ui(ui, |ui| {
                        for &choice in BodyFont::ALL {
                            ui.selectable_value(
                                &mut self.draft.body_font,
                                choice,
                                choice.label_t(),
                            );
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.body_font"));
                if self.draft.body_font != old_body_font {
                    self.font_changed = true;
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.custom_font"))
                    .on_hover_text(crate::i18n::t("settings_hint.custom_font"));
                let old_path = self.draft.custom_font_path.clone();
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.custom_font_path)
                            .hint_text(crate::i18n::t("settings_hint.custom_font_placeholder"))
                            .desired_width(220.0),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.custom_font"));
                    if ui.button(crate::i18n::t("dialog.swb_browse")).clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter("Font (.ttf, .otf, .ttc)", &["ttf", "otf", "ttc"])
                            .pick_file()
                    {
                        self.draft.custom_font_path = p.to_string_lossy().into_owned();
                    }
                    if !self.draft.custom_font_path.is_empty()
                        && ui.button(crate::i18n::t("settings.clear")).clicked()
                    {
                        self.draft.custom_font_path.clear();
                    }
                });
                if self.draft.custom_font_path != old_path {
                    self.font_changed = true;
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.icon_color"))
                    .on_hover_text(crate::i18n::t("settings_hint.icon_color"));
                let old_icon = self.draft.icon_variant;
                ui.horizontal(|ui| {
                    paint_icon_swatch(ui, self.draft.icon_variant.preview_color());
                    egui::ComboBox::from_id_salt("icon_combo")
                        .selected_text(self.draft.icon_variant.label())
                        .show_ui(ui, |ui| {
                            for &variant in IconVariant::ALL {
                                ui.horizontal(|ui| {
                                    paint_icon_swatch(ui, variant.preview_color());
                                    ui.selectable_value(
                                        &mut self.draft.icon_variant,
                                        variant,
                                        variant.label(),
                                    );
                                });
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("settings_hint.icon_color"));
                });
                if self.draft.icon_variant != old_icon {
                    self.icon_changed = true;
                }
                ui.end_row();

                ui.label(crate::i18n::t("settings.window_controls"))
                    .on_hover_text(crate::i18n::t("settings_hint.window_controls"));
                ui.checkbox(&mut self.draft.use_custom_title_bar, "")
                    .on_hover_text(crate::i18n::t("settings_hint.window_controls"));
                ui.end_row();

                // No "off" switch by design: a message that never expires
                // covers the status bar until restart. The floor lives in
                // `AppSettings::status_message_duration`; the hint says so.
                ui.label(crate::i18n::t("settings.status_message_secs"))
                    .on_hover_text(crate::i18n::t("settings_hint.status_message_secs"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.status_message_secs_buf)
                        .desired_width(120.0)
                        .hint_text("10"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.status_message_secs"));
                ui.end_row();
            });
    }
}
