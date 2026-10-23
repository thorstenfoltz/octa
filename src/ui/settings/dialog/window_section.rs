//! Window section: startup size and position behaviour, whether a file
//! opened from outside goes to the Octa already running, and the finish
//! notification.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn window_section_body(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::wrapping_grid(ui, "settings_window")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                crate::ui::control_row::pin_control_height(ui);
                ui.label(crate::i18n::t("settings.start_maximised"))
                    .on_hover_text(crate::i18n::t("settings_hint.start_maximised"));
                ui.checkbox(&mut self.draft.start_maximized, "")
                    .on_hover_text(crate::i18n::t("settings_hint.start_maximised"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.initial_window_size"))
                    .on_hover_text(crate::i18n::t("settings_hint.initial_window_size"));
                ui.add_enabled_ui(!self.draft.start_maximized, |ui| {
                    egui::ComboBox::from_id_salt("window_size_combo")
                        .selected_text(self.draft.window_size.label())
                        .show_ui(ui, |ui| {
                            for &size in WindowSize::ALL {
                                ui.selectable_value(
                                    &mut self.draft.window_size,
                                    size,
                                    size.label(),
                                );
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("settings_hint.initial_window_size"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.open_in_running_instance"))
                    .on_hover_text(crate::i18n::t("settings_hint.open_in_running_instance"));
                ui.checkbox(&mut self.draft.open_in_running_instance, "")
                    .on_hover_text(crate::i18n::t("settings_hint.open_in_running_instance"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.notify_on_finish"))
                    .on_hover_text(crate::i18n::t("settings_hint.notify_on_finish"));
                crate::ui::control_row::control_row(ui, |ui| {
                    let toggled = ui
                        .checkbox(&mut self.draft.notify_on_finish, "")
                        .on_hover_text(crate::i18n::t("settings_hint.notify_on_finish"))
                        .changed();
                    if toggled && self.draft.notify_on_finish {
                        crate::notify::register_windows_identity(
                            self.draft.icon_variant.resolve().svg_source(),
                        );
                    }
                    let on = self.draft.notify_on_finish;
                    let secs_hint = if on {
                        crate::i18n::t("settings_hint.notify_after_secs")
                    } else {
                        crate::i18n::t("settings_hint.notify_after_secs_off")
                    };
                    ui.add_enabled_ui(on, |ui| {
                        ui.label(crate::i18n::t("settings.notify_after_secs"))
                            .on_hover_text(&secs_hint)
                            .on_disabled_hover_text(&secs_hint);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.notify_after_secs_buf)
                                .desired_width(50.0)
                                .hint_text("5"),
                        )
                        .on_hover_text(&secs_hint)
                        .on_disabled_hover_text(&secs_hint);
                    });
                    if ui
                        .button(crate::i18n::t("settings.notify_test"))
                        .on_hover_text(crate::i18n::t("settings_hint.notify_test"))
                        .clicked()
                    {
                        crate::notify::register_windows_identity(
                            self.draft.icon_variant.resolve().svg_source(),
                        );
                        crate::notify::send("Octa".to_string(), crate::i18n::t("notify.test_body"));
                    }
                });
                ui.end_row();

                let on = self.draft.notify_on_finish;
                let focused_hint = if on {
                    crate::i18n::t("settings_hint.notify_when_focused")
                } else {
                    crate::i18n::t("settings_hint.notify_after_secs_off")
                };
                ui.add_enabled_ui(on, |ui| {
                    ui.label(crate::i18n::t("settings.notify_when_focused"))
                        .on_hover_text(&focused_hint)
                        .on_disabled_hover_text(&focused_hint);
                });
                ui.add_enabled_ui(on, |ui| {
                    ui.checkbox(&mut self.draft.notify_when_focused, "")
                        .on_hover_text(&focused_hint)
                        .on_disabled_hover_text(&focused_hint);
                });
                ui.end_row();
                if self.draft.notify_on_finish
                    && crate::notify::parse_after_secs(&self.notify_after_secs_buf).is_none()
                {
                    ui.label("");
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        crate::i18n::t("settings.notify_after_secs_invalid"),
                    );
                    ui.end_row();
                }
            });
    }
}
