//! One-shot per-file prompt offered when a large CSV/TSV is opened. Lets the
//! user pick between keeping the slow features (column coloring + align mode)
//! or disabling them just for this tab. The answer is stored on `TabState`,
//! never on `AppSettings`, so other files keep their defaults.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

pub(crate) fn render_raw_perf_prompt_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_raw_perf_prompt.as_ref() else {
        return;
    };
    let tab_idx = prompt.tab_idx;
    let file_name = prompt.file_name.clone();
    let mb = prompt.file_size as f64 / (1024.0 * 1024.0);

    let mut keep = false;
    let mut disable = false;
    let dialog_id = egui::Id::new("octa_raw_perf_prompt_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(460.0, 260.0));
    let window = egui::Window::new("octa_raw_perf_prompt")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(460.0)
            .default_height(260.0)
            .min_width(320.0)
            .min_height(160.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("raw_perf_prompt_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(octa::i18n::t("dialog.rawperf_title"))
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
            ui.label(RichText::new(format!("\"{}\" ({:.1} MB)", file_name, mb)).strong());
            ui.add_space(4.0);
            ui.label(octa::i18n::t("dialog.rawperf_body"));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(octa::i18n::t("dialog.rawperf_disable")).clicked() {
                    disable = true;
                }
                if ui.button(octa::i18n::t("dialog.rawperf_keep")).clicked() {
                    keep = true;
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(octa::i18n::t("dialog.rawperf_hint"))
                    .weak()
                    .size(11.0),
            );
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
        keep = true;
    }

    if disable {
        if let Some(tab) = app.tabs.get_mut(tab_idx) {
            tab.raw_color_enabled = false;
            tab.raw_perf_prompt_resolved = true;
        }
        app.pending_raw_perf_prompt = None;
    } else if keep {
        if let Some(tab) = app.tabs.get_mut(tab_idx) {
            tab.raw_perf_prompt_resolved = true;
        }
        app.pending_raw_perf_prompt = None;
    }
}
