//! "Tidy up" dialog. Runs whitespace-trim and/or snake_case-header passes on the
//! active table as one undoable step. Driven by `OctaApp.tidy_up_dialog`.

use eframe::egui;
use egui::RichText;

use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

pub(crate) fn render_tidy_up_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.tidy_up_dialog.is_none() {
        return;
    }
    if app.is_readonly() {
        app.tidy_up_dialog = None;
        return;
    }
    let mut state = app.tidy_up_dialog.take().unwrap();
    let mut close = false;
    let mut apply = false;

    let dialog_id = egui::Id::new("octa_tidy_up_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(state.size));
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_tidy_up")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(false).default_width(360.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("tidy_up_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("tidyup.title"))
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
                RichText::new(octa::i18n::t("tidyup.hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            ui.checkbox(&mut state.trim, octa::i18n::t("tidyup.trim"));
            ui.checkbox(&mut state.headers, octa::i18n::t("tidyup.headers"));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_apply = state.trim || state.headers;
                if ui
                    .add_enabled(can_apply, egui::Button::new(octa::i18n::t("tidyup.apply")))
                    .clicked()
                {
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
        app.apply_tidy_up(state.trim, state.headers);
        return; // state consumed
    }
    if !close {
        state.size = size;
        app.tidy_up_dialog = Some(state);
    }
}
