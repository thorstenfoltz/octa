//! Modal asking how to read a numeric column that is written ambiguously.
//! `1,234` is one thousand two hundred and thirty four in a German file and
//! one point two three four in an English one, and nothing in the column
//! itself resolves that. Multiple such columns queue: the head of
//! `pending_number_pickers` is the active dialog.

use eframe::egui;
use egui::RichText;
use octa::data::num_parse::{self, NumberStyle};

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

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

    let dialog_id = egui::Id::new("octa_number_ambiguity_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(480.0, 320.0));
    let window = egui::Window::new("octa_number_ambiguity")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(320.0)
            .min_width(340.0)
            .min_height(200.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("number_ambiguity_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.num_title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
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
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        choice = Some(None);
    }

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
