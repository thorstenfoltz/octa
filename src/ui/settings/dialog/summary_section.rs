//! Summary section: which statistics the Summary tab offers.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn summary_section_body(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(crate::i18n::t("settings_hint.summary_intro"))
                .weak()
                .size(11.0),
        );
        ui.add_space(6.0);
        egui::Grid::new("settings_summary")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                use crate::data::summary::SummaryStat;
                for stat in SummaryStat::all() {
                    // Column name + type are always shown, so there is
                    // nothing to toggle: leave them out of the list
                    // entirely (the intro note explains they are included).
                    if stat.is_mandatory() {
                        continue;
                    }
                    // Label each row by the exact column id the user will
                    // see in the Summary table, with the localized
                    // description on hover.
                    ui.label(stat.column_id())
                        .on_hover_text(crate::i18n::t(stat.hint_key()));
                    let mut on = self.draft.summary_stats.contains(&stat);
                    let toggle = ui
                        .checkbox(&mut on, "")
                        .on_hover_text(crate::i18n::t(stat.hint_key()));
                    if toggle.changed() {
                        if on {
                            if !self.draft.summary_stats.contains(&stat) {
                                self.draft.summary_stats.push(stat);
                            }
                        } else {
                            self.draft.summary_stats.retain(|s| *s != stat);
                        }
                    }
                    ui.end_row();
                }
            });
    }
}
