//! Report dialog: build a self-contained HTML profiling report of the active
//! tab.
//!
//! Reached from **File -> Report...**. The build runs on a worker thread
//! with a polled result slot, modelled on `schema_drift.rs`: a full pass over
//! a wide table takes seconds and must not sit on the UI thread.
//!
//! The engine (`octa::data::report`) is the same one the CLI `--report` flag
//! and the `create_report` tool use, so all three produce the same document.

use eframe::egui;
use egui::RichText;

use octa::data::report::{ReportOptions, ReportSection, build_report};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, ReportState};

use std::sync::atomic::Ordering;

/// Label and tooltip keys for one section checkbox.
fn section_keys(s: ReportSection) -> (&'static str, &'static str) {
    match s {
        ReportSection::Stats => ("report.sec_stats", "report.sec_stats_hint"),
        ReportSection::Distributions => {
            ("report.sec_distributions", "report.sec_distributions_hint")
        }
        ReportSection::TopValues => ("report.sec_top_values", "report.sec_top_values_hint"),
        ReportSection::Correlation => ("report.sec_correlation", "report.sec_correlation_hint"),
    }
}

pub(crate) fn render_report_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.report_dialog.is_none() {
        return;
    }

    let mut close = false;
    let mut create = false;
    let mut pick_file = false;
    let mut open_now = false;
    let mut st = app.report_dialog.take().unwrap();

    let running = st.running.load(Ordering::Relaxed);

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_report_dialog_v1");
    let window = egui::Window::new("octa_report")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(360.0)
            .min_width(380.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("report_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("report.title"))
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

        egui::Panel::bottom("report_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(octa::i18n::t("report.running"));
                        if ui.button(octa::i18n::t("report.cancel")).clicked() {
                            st.cancel.store(true, Ordering::Relaxed);
                        }
                    } else if st.done_path.is_some() {
                        if ui
                            .button(octa::i18n::t("report.open"))
                            .on_hover_text(octa::i18n::t("report.open_hint"))
                            .clicked()
                        {
                            open_now = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("report.cancel")).clicked() {
                                close = true;
                            }
                        });
                    } else {
                        let ready = !st.destination.trim().is_empty() && !st.sections.is_empty();
                        if ui
                            .add_enabled(ready, egui::Button::new(octa::i18n::t("report.create")))
                            .on_hover_text(octa::i18n::t("report.create_hint"))
                            .on_disabled_hover_text(octa::i18n::t("report.destination_hint"))
                            .clicked()
                        {
                            create = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(octa::i18n::t("report.cancel")).clicked() {
                                close = true;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(done) = &st.done_path {
                ui.label(
                    octa::i18n::t("report.done").replace("{path}", &done.display().to_string()),
                );
                ui.separator();
            }
            ui.label(RichText::new(octa::i18n::t("report.sections")).strong());
            for section in ReportSection::ALL {
                let (label, hint) = section_keys(*section);
                let mut on = st.sections.contains(section);
                if ui
                    .checkbox(&mut on, octa::i18n::t(label))
                    .on_hover_text(octa::i18n::t(hint))
                    .changed()
                {
                    if on {
                        // Keep the canonical order rather than click order, so
                        // the document always reads the same way.
                        st.sections = ReportSection::ALL
                            .iter()
                            .copied()
                            .filter(|s| s == section || st.sections.contains(s))
                            .collect();
                    } else {
                        st.sections.retain(|s| s != section);
                    }
                }
            }

            ui.add_space(8.0);
            ui.checkbox(&mut st.sample_enabled, octa::i18n::t("report.sample"))
                .on_hover_text(octa::i18n::t("report.sample_hint"));
            if st.sample_enabled {
                ui.horizontal(|ui| {
                    ui.label(octa::i18n::t("report.sample_rows"));
                    ui.add(
                        egui::TextEdit::singleline(&mut st.sample_rows_text).desired_width(90.0),
                    )
                    .on_hover_text(octa::i18n::t("report.sample_hint"));
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(octa::i18n::t("report.destination"))
                    .on_hover_text(octa::i18n::t("report.destination_hint"));
                if ui
                    .button(octa::i18n::t("report.browse"))
                    .on_hover_text(octa::i18n::t("report.destination_hint"))
                    .clicked()
                {
                    pick_file = true;
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut st.destination)
                    .desired_width(f32::INFINITY)
                    .hint_text(octa::i18n::t("report.destination_hint")),
            );
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    // The file picker blocks, so it runs after the frame is laid out.
    if pick_file
        && let Some(path) = rfd::FileDialog::new()
            .add_filter("HTML", &["html"])
            .set_file_name("report.html")
            .save_file()
    {
        st.destination = path.display().to_string();
    }

    if close && !running {
        return; // dialog dropped
    }
    app.report_dialog = Some(st);
    if open_now {
        app.open_last_report();
    }
    if create {
        start_build(app, ctx);
    }
}

/// Open the Report dialog for the active tab, defaulting the destination to
/// the chat export folder so the field is never blank.
pub(crate) fn open_report_dialog(app: &mut OctaApp) {
    let name = crate::app::chat_panel::helpers::tab_display_name(
        &app.tabs[app.active_tab],
        app.active_tab,
    );
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let dest =
        std::path::Path::new(&app.settings.chat_export_dir).join(format!("{stem}-report.html"));
    app.report_dialog = Some(ReportState::new(dest.display().to_string()));
}

fn start_build(app: &mut OctaApp, ctx: &egui::Context) {
    // Snapshot the tab first: the worker must not borrow app state, and the
    // report should reflect the edits on screen (same as open_describe_tab).
    let tab = &mut app.tabs[app.active_tab];
    tab.table.apply_edits();
    let snapshot = tab.table.clone();
    let rows: Vec<usize> = tab.filtered_rows.clone();
    let title = crate::app::chat_panel::helpers::tab_display_name(tab, app.active_tab);

    let Some(st) = &mut app.report_dialog else {
        return;
    };
    let sample_rows = st.sample_enabled.then(|| {
        st.sample_rows_text
            .replace([',', '.', ' '], "")
            .parse::<usize>()
            .unwrap_or(10_000)
    });
    let opts = ReportOptions {
        sections: st.sections.clone(),
        sample_rows,
        title,
        ..ReportOptions::default()
    };
    let dest = std::path::PathBuf::from(st.destination.trim());

    st.done_path = None;
    st.cancel.store(false, Ordering::Relaxed);
    st.running.store(true, Ordering::Relaxed);

    let running = std::sync::Arc::clone(&st.running);
    let cancel = std::sync::Arc::clone(&st.cancel);
    let result = std::sync::Arc::clone(&st.result);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let outcome = build_report(&snapshot, &rows, &opts, &cancel)
            .and_then(|html| {
                std::fs::write(&dest, html)?;
                Ok(dest.clone())
            })
            .map_err(|e| e.to_string());
        if let Ok(mut slot) = result.lock() {
            *slot = Some(outcome);
        }
        running.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}

impl OctaApp {
    /// Drain a finished report build: close the dialog and report where it went.
    pub(crate) fn drain_report(&mut self) {
        let Some(st) = &self.report_dialog else {
            return;
        };
        let outcome = match st.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => None,
        };
        let Some(outcome) = outcome else {
            return;
        };

        match outcome {
            Ok(path) => {
                self.last_report_path = Some(path.clone());
                if let Some(st) = &mut self.report_dialog {
                    st.done_path = Some(path.clone());
                }
                self.status_message = Some((
                    octa::i18n::t("report.done").replace("{path}", &path.display().to_string()),
                    std::time::Instant::now(),
                ));
            }
            Err(reason) => {
                // Keep the dialog open: the destination or the section choice
                // is usually what needs changing.
                self.status_message = Some((
                    octa::i18n::t("report.failed").replace("{error}", &reason),
                    std::time::Instant::now(),
                ));
            }
        }
    }

    /// Open the finished report in the user's browser.
    pub(crate) fn open_last_report(&mut self) {
        let Some(path) = self.last_report_path.clone() else {
            return;
        };
        let url = match path.canonicalize() {
            Ok(abs) => format!("file://{}", abs.display()),
            Err(_) => format!("file://{}", path.display()),
        };
        octa::auth::oauth_browser::open_url_in_browser(&url);
    }
}
