//! Schema drift dialog: which files in a folder disagree about their columns.
//!
//! Reached from **File -> Schema drift...** or a folder's sidebar context
//! menu (**Scan schemas...**, which prefills the folder). The scan runs on a
//! worker thread and the dialog polls a shared slot per frame, modelled on
//! `batch_convert.rs`: reading several hundred file footers blocks for
//! seconds and must never sit on the UI thread.
//!
//! The engine (`octa::data::schema_drift`) is the same one the CLI
//! `--schema-drift` flag and the `schema_drift` MCP tool run.

use eframe::egui;
use egui::RichText;

use octa::data::schema_drift::{DriftOptions, analyse, collect_schemas, report_table};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, SchemaDriftState};

use std::sync::atomic::Ordering;

pub(crate) fn render_schema_drift_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.schema_drift_dialog.is_none() {
        return;
    }

    let mut close = false;
    let mut scan = false;
    let mut harmonise = false;
    let mut pick_dir = false;
    let mut st = app.schema_drift_dialog.take().unwrap();

    let running = st.running.load(Ordering::Relaxed);

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_schema_drift_dialog_v1");
    let window = egui::Window::new("octa_schema_drift")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(260.0)
            .min_width(360.0)
            .min_height(160.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("schema_drift_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("drift.title"))
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

        egui::Panel::bottom("schema_drift_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(octa::i18n::t("drift.running"));
                    } else {
                        let ready = !st.folder.trim().is_empty();
                        if ui
                            .add_enabled(ready, egui::Button::new(octa::i18n::t("drift.scan")))
                            .on_hover_text(octa::i18n::t("drift.scan_hint"))
                            .on_disabled_hover_text(octa::i18n::t("drift.folder_hint"))
                            .clicked()
                        {
                            scan = true;
                        }
                        // Hand off to the write half. It runs its own scan (it
                        // needs the majority variant as the target anyway), so
                        // this carries the folder and options rather than a
                        // report, and works whether or not a scan has run yet.
                        if ui
                            .add_enabled(ready, egui::Button::new(octa::i18n::t("drift.harmonise")))
                            .on_hover_text(octa::i18n::t("drift.harmonise_hint"))
                            .on_disabled_hover_text(octa::i18n::t("drift.folder_hint"))
                            .clicked()
                        {
                            harmonise = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("drift.cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("drift.folder"))
                    .on_hover_text(octa::i18n::t("drift.folder_hint"));
                if ui
                    .button(octa::i18n::t("drift.browse"))
                    .on_hover_text(octa::i18n::t("drift.folder_hint"))
                    .clicked()
                {
                    pick_dir = true;
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut st.folder)
                    .desired_width(f32::INFINITY)
                    .hint_text(octa::i18n::t("drift.folder_hint")),
            );

            ui.add_space(8.0);
            ui.checkbox(&mut st.recursive, octa::i18n::t("drift.recursive"))
                .on_hover_text(octa::i18n::t("drift.recursive_hint"));
            ui.checkbox(&mut st.ignore_case, octa::i18n::t("drift.ignore_case"))
                .on_hover_text(octa::i18n::t("drift.ignore_case_hint"));
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    // The folder picker blocks, so it runs after the frame is laid out.
    if pick_dir && let Some(dir) = rfd::FileDialog::new().pick_folder() {
        st.folder = dir.display().to_string();
    }

    // Hand off to the harmonise dialog, carrying the folder and scan options
    // so the user does not retype them. This dialog closes.
    if harmonise {
        super::harmonise::open_for_folder(app, std::path::Path::new(st.folder.trim()));
        if let Some(h) = app.harmonise_dialog.as_mut() {
            h.recursive = st.recursive;
            h.ignore_case = st.ignore_case;
        }
        return;
    }

    if close && !running {
        return; // dialog dropped
    }
    app.schema_drift_dialog = Some(st);
    if scan {
        start_scan(app, ctx);
    }
}

fn start_scan(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(st) = &mut app.schema_drift_dialog else {
        return;
    };
    let dir = std::path::PathBuf::from(st.folder.trim());
    let opts = DriftOptions {
        ignore_case: st.ignore_case,
        recursive: st.recursive,
    };
    st.running.store(true, Ordering::Relaxed);

    let running = std::sync::Arc::clone(&st.running);
    let result = std::sync::Arc::clone(&st.result);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let outcome = if dir.is_dir() {
            let registry = octa::formats::FormatRegistry::new();
            let (files, skipped) = collect_schemas(&dir, opts.recursive, &registry);
            if files.is_empty() {
                Err(octa::i18n::t("drift.empty"))
            } else {
                let mut report = analyse(&files, &opts);
                report.skipped = skipped;
                Ok(report)
            }
        } else {
            Err(octa::i18n::t("drift.empty"))
        };
        if let Ok(mut slot) = result.lock() {
            *slot = Some(outcome);
        }
        running.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}

impl OctaApp {
    /// Turn a finished scan into a report tab and close the dialog.
    pub(crate) fn drain_schema_drift(&mut self) {
        let Some(st) = &self.schema_drift_dialog else {
            return;
        };
        let outcome = match st.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => None,
        };
        let Some(outcome) = outcome else {
            return;
        };

        let report = match outcome {
            Ok(report) => report,
            Err(reason) => {
                self.status_message = Some((reason, std::time::Instant::now()));
                self.schema_drift_dialog = None;
                return;
            }
        };

        let file_count: usize = report.variants.iter().map(|v| v.files.len()).sum();
        let mut note = if report.has_drift {
            octa::i18n::t("drift.drift_found")
                .replace("{variants}", &report.variants.len().to_string())
                .replace("{count}", &file_count.to_string())
                .replace("{columns}", &report.drifting_columns.join(", "))
        } else {
            octa::i18n::t("drift.no_drift").replace("{count}", &file_count.to_string())
        };
        if !report.skipped.is_empty() {
            note.push(' ');
            note.push_str(
                &octa::i18n::t("drift.skipped")
                    .replace("{count}", &report.skipped.len().to_string()),
            );
        }

        let mut tab = super::super::state::TabState::new(self.settings.default_search_mode);
        tab.table = report_table(&report);
        tab.custom_tab_label = Some(octa::i18n::t("drift.tab_label"));
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.schema_drift_dialog = None;
        self.status_message = Some((note, std::time::Instant::now()));
    }
}

/// Open the dialog prefilled with `dir` (the sidebar's folder context menu).
pub(crate) fn open_for_folder(app: &mut OctaApp, dir: &std::path::Path) {
    app.schema_drift_dialog = Some(SchemaDriftState::new(dir.display().to_string()));
}
