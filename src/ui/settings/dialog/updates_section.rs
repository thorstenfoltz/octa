//! Updates section: check-on-start and show-release-notes toggles.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn updates_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_updates")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                // A Store (MSIX) copy never runs the check, so the switch for
                // it is greyed out and says who does the updating instead.
                let store = crate::platform::is_store_packaged();
                ui.label(crate::i18n::t("release.check_on_start"))
                    .on_hover_text(crate::i18n::t("release.check_on_start_hint"));
                ui.add_enabled(
                    !store,
                    egui::Checkbox::new(&mut self.draft.check_updates_on_start, ""),
                )
                .on_hover_text(crate::i18n::t("release.check_on_start_hint"))
                .on_disabled_hover_text(crate::i18n::t("release.store"));
                ui.end_row();

                ui.label(crate::i18n::t("release.show_notes"))
                    .on_hover_text(crate::i18n::t("release.show_notes_hint"));
                ui.checkbox(&mut self.draft.show_release_notes, "")
                    .on_hover_text(crate::i18n::t("release.show_notes_hint"));
                ui.end_row();
            });
    }
}
