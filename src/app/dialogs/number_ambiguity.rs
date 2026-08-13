//! Modal asking how to read a numeric column that is written ambiguously.
//! `1,234` is one thousand two hundred and thirty four in a German file and
//! one point two three four in an English one, and nothing in the column
//! itself resolves that. Multiple such columns queue: the head of
//! `pending_number_pickers` is the active dialog.

use eframe::egui;
use egui::RichText;
use octa::data::num_parse::{self, NumberStyle};

use super::super::state::OctaApp;

pub(crate) fn render_number_ambiguity_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.pending_number_pickers.front() else {
        return;
    };
    let col_name = state.col_name.clone();
    let samples = state.samples.clone();
    let tab_idx = state.tab_idx;
    let col_idx = state.col_idx;

    // `Some(style)` converts, `None` leaves the column as text.
    let mut choice: Option<Option<NumberStyle>> = None;

    egui::Window::new(octa::i18n::t("dialog.num_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "{}: '{}'",
                    octa::i18n::t("dialog.num_column"),
                    col_name
                ))
                .strong(),
            );
            ui.add_space(4.0);
            ui.label(octa::i18n::t("dialog.num_body"));
            ui.add_space(8.0);
            ui.label(RichText::new(octa::i18n::t("dialog.num_samples")).strong());
            for s in &samples {
                ui.label(RichText::new(format!("  {s}")).monospace());
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            if ui
                .button(NumberStyle::European.label())
                .on_hover_text(octa::i18n::t("dialog.num_european_hint"))
                .clicked()
            {
                choice = Some(Some(NumberStyle::European));
            }
            if ui
                .button(NumberStyle::English.label())
                .on_hover_text(octa::i18n::t("dialog.num_english_hint"))
                .clicked()
            {
                choice = Some(Some(NumberStyle::English));
            }
            ui.add_space(8.0);
            if ui
                .button(octa::i18n::t("dialog.num_leave_as_text"))
                .on_hover_text(octa::i18n::t("dialog.num_leave_as_text_hint"))
                .clicked()
            {
                choice = Some(None);
            }
        });

    if let Some(answer) = choice {
        if let Some(style) = answer
            && tab_idx < app.tabs.len()
        {
            let tab = &mut app.tabs[tab_idx];
            num_parse::apply_style(&mut tab.table, col_idx, style);
            tab.filter_dirty = true;
            tab.table_state.invalidate_row_heights();
        }
        app.pending_number_pickers.pop_front();
    }
}
