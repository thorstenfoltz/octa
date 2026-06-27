//! Correlation-matrix dialog: pick Pearson/Spearman, compute over all numeric
//! columns (the engine decides which), open the matrix as a detached tab.

use eframe::egui;
use egui::RichText;

use octa::data::correlation::CorrMethod;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::OctaApp;

pub(crate) fn render_correlation_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.correlation_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut st = app.correlation_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_correlation_dialog");
    let window = egui::Window::new("octa_correlation")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(360.0)
            .default_height(180.0)
            .min_width(280.0)
            .min_height(140.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("correlation_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("dialog.corr_title"))
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
        egui::Panel::bottom("correlation_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("dialog.corr_compute")).clicked() {
                        run = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(octa::i18n::t("dialog.corr_intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            ui.radio_value(
                &mut st.method,
                CorrMethod::Pearson,
                octa::i18n::t("dialog.corr_pearson"),
            );
            ui.radio_value(
                &mut st.method,
                CorrMethod::Spearman,
                octa::i18n::t("dialog.corr_spearman"),
            );
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        app.open_correlation_tab(st.method);
        return;
    }
    if !close {
        app.correlation_dialog = Some(st);
    }
}
