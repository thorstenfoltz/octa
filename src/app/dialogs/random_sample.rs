//! "Random sample" dialog. Asks for a row count, then opens a detached tab of
//! that many randomly chosen rows from the active table. Driven by
//! `OctaApp.random_sample_dialog`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_random_sample_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.random_sample_dialog.is_none() {
        return;
    }
    let mut state = app.random_sample_dialog.take().unwrap();
    let mut close = false;
    let mut apply = false;

    let dialog_id = egui::Id::new("octa_random_sample_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_random_sample")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(false).default_width(340.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("random_sample_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("sample.title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });

        if minimized {
            return;
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("sample.hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("sample.rows"));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.n_buf)
                        .desired_width(100.0)
                        .hint_text("100"),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter {
                    apply = true;
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("sample.create")).clicked() {
                    apply = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(octa::i18n::t("common.cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || apply {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if apply {
        // Parse the count; a blank or unparseable value defaults to 100. Zero
        // and huge values are clamped by the builder against the row count.
        let n = state
            .n_buf
            .trim()
            .replace([',', '.', ' '], "")
            .parse::<usize>()
            .unwrap_or(100)
            .max(1);
        app.open_random_sample_tab(n);
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.random_sample_dialog = Some(state);
    }
}
