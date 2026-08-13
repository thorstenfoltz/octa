//! Batch convert dialog: N files into one target format in a single run.
//!
//! Reached from the sidebar (select files, then **Convert...**) or from
//! **File -> Batch convert...** (pick a folder). The work runs on a worker
//! thread holding the plan; the dialog polls the shared progress/result slots
//! per frame, exactly like the clean-up panel's scan.
//!
//! The engine (`octa::data::batch_convert`) is the same one the CLI
//! `--batch-convert` flag and the `batch_convert` MCP tool run.

use eframe::egui;
use egui::RichText;

use octa::data::batch_convert::{plan_batch, run_batch};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, render_write_options,
    size_dialog_window,
};

use crate::app::state::{BatchConvertState, OctaApp};

use std::sync::atomic::Ordering;

/// How many input paths the dialog lists before summarising the rest.
const SHOWN_INPUTS: usize = 6;

/// Extensions offered as conversion targets: every registry format that can
/// actually write, so the picker can never propose an impossible target.
fn writable_targets() -> Vec<String> {
    let registry = octa::formats::FormatRegistry::new();
    let mut out: Vec<String> = registry
        .all_extensions()
        .into_iter()
        .filter(|ext| {
            registry
                .reader_for_path(std::path::Path::new(&format!("_check_.{ext}")))
                .map(|r| r.supports_write())
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn render_batch_convert_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.batch_convert_dialog.is_none() {
        return;
    }

    let mut close = false;
    let mut run = false;
    let mut pick_dir = false;
    let mut st = app.batch_convert_dialog.take().unwrap();

    let running = st.running.load(Ordering::Relaxed);
    let done = st.progress.load(Ordering::Relaxed);
    let targets = writable_targets();

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_batch_convert_dialog");
    let window = egui::Window::new("octa_batch_convert")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(420.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("batch_convert_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("batch.title"))
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

        egui::Panel::bottom("batch_convert_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(
                            octa::i18n::t("batch.running")
                                .replace("{done}", &done.to_string())
                                .replace("{total}", &st.total.to_string()),
                        );
                        if ui.button(octa::i18n::t("batch.cancel")).clicked() {
                            st.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        let ready = st.out_dir.is_some() && !st.inputs.is_empty();
                        if ui
                            .add_enabled(ready, egui::Button::new(octa::i18n::t("batch.run")))
                            .clicked()
                        {
                            run = true;
                        }
                        if !ready {
                            ui.label(
                                RichText::new(octa::i18n::t("batch.need_dir"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("common.cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(
                    octa::i18n::t("batch.inputs").replace("{n}", &st.inputs.len().to_string()),
                )
                .strong(),
            );
            egui::ScrollArea::vertical()
                .id_salt("batch_inputs")
                .auto_shrink([false, true])
                .max_height(120.0)
                .show(ui, |ui| {
                    for path in st.inputs.iter().take(SHOWN_INPUTS) {
                        ui.weak(
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.display().to_string()),
                        );
                    }
                    if st.inputs.len() > SHOWN_INPUTS {
                        ui.weak(format!("... {}", st.inputs.len() - SHOWN_INPUTS));
                    }
                });

            ui.add_space(6.0);
            egui::Grid::new("batch_convert_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(octa::i18n::t("batch.target"));
                    egui::ComboBox::from_id_salt("batch_target")
                        .selected_text(&st.target_ext)
                        .show_ui(ui, |ui| {
                            for ext in &targets {
                                ui.selectable_value(&mut st.target_ext, ext.clone(), ext);
                            }
                        });
                    ui.end_row();

                    ui.label(octa::i18n::t("batch.out_dir"));
                    ui.horizontal(|ui| {
                        if ui.button(octa::i18n::t("batch.choose_dir")).clicked() {
                            pick_dir = true;
                        }
                        if let Some(dir) = &st.out_dir {
                            ui.weak(dir.display().to_string());
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.checkbox(&mut st.overwrite, octa::i18n::t("batch.overwrite"));

            ui.add_space(4.0);
            render_write_options(ui, &mut st.write_options, &mut st.row_group_buf);
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    // The folder picker blocks, so it runs after the frame is laid out.
    if pick_dir && let Some(dir) = rfd::FileDialog::new().pick_folder() {
        st.out_dir = Some(dir);
    }

    if close && !running {
        return; // dialog dropped
    }
    app.batch_convert_dialog = Some(st);
    if run {
        start_run(app, ctx);
    }
}

fn start_run(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(st) = &mut app.batch_convert_dialog else {
        return;
    };
    let Some(out_dir) = st.out_dir.clone() else {
        return;
    };
    let plan = plan_batch(&st.inputs, &out_dir, &st.target_ext, st.overwrite);
    st.total = plan.len();
    st.progress.store(0, Ordering::Relaxed);
    st.cancel.store(false, Ordering::Relaxed);
    st.running.store(true, Ordering::Relaxed);

    let progress = std::sync::Arc::clone(&st.progress);
    let running = std::sync::Arc::clone(&st.running);
    let cancel = std::sync::Arc::clone(&st.cancel);
    let result = std::sync::Arc::clone(&st.result);
    // Write options for this run: the dialog's copy, seeded from Settings.
    let write_opts = st.write_options.clone();
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let report = run_batch(
            plan,
            &|done, _total| progress.store(done, Ordering::Relaxed),
            &cancel,
            &write_opts,
        );
        if let Ok(mut slot) = result.lock() {
            *slot = Some(report);
        }
        running.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}

impl OctaApp {
    /// Turn a finished batch run into a report tab and close the dialog.
    pub(crate) fn drain_batch_convert(&mut self) {
        let Some(st) = &self.batch_convert_dialog else {
            return;
        };
        let report = match st.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => None,
        };
        let Some(report) = report else {
            return;
        };
        let mut tab = super::super::state::TabState::new(self.settings.default_search_mode);
        tab.table = octa::data::batch_convert::report_table(&report);
        tab.custom_tab_label = Some(octa::i18n::t("batch.report_tab"));
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.batch_convert_dialog = None;
        self.status_message = Some((
            octa::i18n::t("batch.done")
                .replace("{ok}", &report.converted.to_string())
                .replace("{fail}", &report.failed.to_string())
                .replace("{skip}", &report.skipped.to_string()),
            std::time::Instant::now(),
        ));
    }
}

/// Open the dialog for every readable file directly inside `dir`.
pub(crate) fn open_for_folder(app: &mut OctaApp, dir: &std::path::Path) {
    let files: Vec<std::path::PathBuf> = octa::ui::directory_tree::read_sorted_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    if files.is_empty() {
        app.status_message = Some((octa::i18n::t("batch.no_files"), std::time::Instant::now()));
        return;
    }
    app.batch_convert_dialog =
        Some(BatchConvertState::new(files).with_write_options(app.settings.write_options.clone()));
}
