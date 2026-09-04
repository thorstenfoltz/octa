//! MCP section: row limit and per-cell byte cap for the `--mcp` server.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn mcp_section_body(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(crate::i18n::t("settings_hint.mcp_intro"))
                .weak()
                .size(11.0),
        );
        ui.add_space(6.0);
        egui::Grid::new("settings_mcp")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.default_row_limit"))
                    .on_hover_text(crate::i18n::t("settings_hint.mcp_row_limit"));
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.mcp_row_limit_buf)
                        .desired_width(100.0)
                        .hint_text("1,000");
                    ui.add_enabled(!self.mcp_unlimited_rows, edit)
                        .on_hover_text(crate::i18n::t("settings_hint.mcp_row_limit"))
                        .on_disabled_hover_text(crate::i18n::t("settings_hint.mcp_row_limit"));
                    ui.checkbox(
                        &mut self.mcp_unlimited_rows,
                        crate::i18n::t("settings.unlimited"),
                    );
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.cell_byte_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.cell_byte_cap"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.mcp_cell_bytes_buf)
                        .desired_width(120.0)
                        .hint_text("65,536"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.cell_byte_cap"));
                ui.end_row();
            });
    }
}
