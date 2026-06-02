//! Modal that asks the user how to interpret an ambiguous date column. Fires
//! once per column whose values match more than one date layout (e.g.
//! `02/03/2024` is consistent with both DD/MM and MM/DD). Multiple ambiguous
//! columns queue: the head of `pending_date_pickers` is the active dialog.

use eframe::egui;
use egui::RichText;
use octa::data::date_infer::{self, DateLayout, DateTimeLayout};

use super::super::state::OctaApp;

#[derive(Clone)]
enum Choice {
    Date(DateLayout),
    DateTime(DateTimeLayout),
    Skip,
}

pub(crate) fn render_date_ambiguity_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.pending_date_pickers.front() else {
        return;
    };

    let title = octa::i18n::t("dialog.date_title");
    let col_name = state.col_name.clone();
    let samples = state.samples.clone();
    let date_candidates = state.date_candidates.clone();
    let datetime_candidates = state.datetime_candidates.clone();
    let tab_idx = state.tab_idx;
    let col_idx = state.col_idx;

    let mut choice: Option<Choice> = None;

    egui::Window::new(title)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "{}: '{}'",
                    octa::i18n::t("dialog.date_column"),
                    col_name
                ))
                .strong(),
            );
            ui.add_space(4.0);
            ui.label(octa::i18n::t("dialog.date_body"));
            ui.add_space(8.0);
            ui.label(RichText::new(octa::i18n::t("dialog.date_samples")).strong());
            for s in &samples {
                ui.label(RichText::new(format!("  {s}")).monospace());
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            for layout in &date_candidates {
                if ui.button(layout.label()).clicked() {
                    choice = Some(Choice::Date(*layout));
                }
            }
            for layout in &datetime_candidates {
                if ui.button(layout.label()).clicked() {
                    choice = Some(Choice::DateTime(*layout));
                }
            }
            ui.add_space(8.0);
            if ui
                .button(octa::i18n::t("dialog.date_leave_as_text"))
                .clicked()
            {
                choice = Some(Choice::Skip);
            }
        });

    if let Some(c) = choice {
        if tab_idx < app.tabs.len() {
            let tab = &mut app.tabs[tab_idx];
            match c {
                Choice::Date(layout) => date_infer::apply_date(&mut tab.table, col_idx, layout),
                Choice::DateTime(layout) => {
                    date_infer::apply_datetime(&mut tab.table, col_idx, layout)
                }
                Choice::Skip => {}
            }
            tab.filter_dirty = true;
            tab.table_state.invalidate_row_heights();
        }
        app.pending_date_pickers.pop_front();
    }
}
