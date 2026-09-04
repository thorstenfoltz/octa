//! Diagnostics section: debug logging and the redacted debug report.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn diagnostics_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_diagnostics")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("diagnostics.debug_mode"))
                    .on_hover_text(crate::i18n::t("diagnostics.debug_mode_hint"));
                ui.checkbox(&mut self.draft.debug_mode, "")
                    .on_hover_text(crate::i18n::t("diagnostics.debug_mode_hint"));
                ui.end_row();
            });
        if ui
            .button(crate::i18n::t("diagnostics.open_log_folder"))
            .on_hover_text(crate::i18n::t("diagnostics.open_log_folder_hint"))
            .clicked()
            && let Some(dir) = crate::diagnostics::logs_dir()
        {
            // Ensure it exists so the file manager has something to show
            // even before the first log line is written.
            let _ = std::fs::create_dir_all(&dir);
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(&dir).spawn();
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
    }
}
