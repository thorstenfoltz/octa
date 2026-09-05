//! Map section: default mode, tile URL template and the geometry fallback.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;
use crate::data::MapMode;

impl SettingsDialog {
    pub(super) fn map_section_body(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(crate::i18n::t("settings_hint.map_intro"))
                .weak()
                .size(11.0),
        );
        ui.add_space(6.0);
        egui::Grid::new("settings_map")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.map_default_mode"))
                    .on_hover_text(crate::i18n::t("settings_hint.map_default_mode"));
                egui::ComboBox::from_id_salt("map_default_mode_combo")
                    .selected_text(self.draft.map_default_mode.label_t())
                    .show_ui(ui, |ui| {
                        for &m in MapMode::ALL {
                            ui.selectable_value(&mut self.draft.map_default_mode, m, m.label_t());
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.map_default_mode"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.map_fallback"))
                    .on_hover_text(crate::i18n::t("settings_hint.map_fallback"));
                ui.checkbox(&mut self.draft.map_fallback_to_geometry, "")
                    .on_hover_text(crate::i18n::t("settings_hint.map_fallback"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.tile_url"))
                    .on_hover_text(crate::i18n::t("settings_hint.tile_url"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.map_tile_url_template)
                        .desired_width(380.0)
                        .hint_text("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.tile_url"));
                ui.end_row();
            });
    }
}
