//! One-shot per-file prompt offered when a large CSV/TSV is opened. Lets the
//! user pick between keeping the slow features (column coloring + align mode)
//! or disabling them just for this tab. The answer is stored on `TabState`,
//! never on `AppSettings`, so other files keep their defaults.

use eframe::egui;
use egui::RichText;

use super::super::state::OctaApp;

pub(crate) fn render_raw_perf_prompt_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_raw_perf_prompt.as_ref() else {
        return;
    };
    let tab_idx = prompt.tab_idx;
    let file_name = prompt.file_name.clone();
    let mb = prompt.file_size as f64 / (1024.0 * 1024.0);

    let mut keep = false;
    let mut disable = false;
    egui::Window::new("Large CSV file")
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("\"{}\" is {:.1} MB.", file_name, mb)).strong());
            ui.add_space(4.0);
            ui.label(
                "Per-column coloring re-tokenizes the whole file on every \
                 layout pass, which is what makes raw view sluggish at this \
                 size. Disable coloring for this file? Align Columns stays \
                 available either way.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Disable coloring for this file").clicked() {
                    disable = true;
                }
                if ui.button("Keep coloring on").clicked() {
                    keep = true;
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("This choice only affects the current tab.")
                    .weak()
                    .size(11.0),
            );
        });

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
