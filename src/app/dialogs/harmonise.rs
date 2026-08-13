//! Harmonise schemas dialog: rewrite a folder of drifting files to one shape.
//!
//! The write half of the schema-drift scan, reached from **File -> Harmonise
//! schemas...** or from the drift dialog's **Harmonise...** button, which
//! prefills the folder and the largest variant as the target.
//!
//! Two phases, both on worker threads like `schema_drift.rs`: **Plan** scans
//! and reports what would happen, **Run** writes. The split is not ceremony.
//! Columns absent from the target get dropped, which is the only lossy part of
//! the operation, and the user has to be able to see that before committing
//! rather than read about it in the report afterwards.
//!
//! The engine (`octa::data::harmonise`) is the same one the CLI
//! `--harmonise-schema` flag and the `harmonise_schemas` MCP tool run.

use eframe::egui;
use egui::RichText;

use octa::data::harmonise::{HarmoniseOptions, plan_harmonise, report_table, run_harmonise};
use octa::data::schema_drift::collect_schemas;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{HarmoniseState, OctaApp};

use std::sync::atomic::Ordering;

pub(crate) fn render_harmonise_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.harmonise_dialog.is_none() {
        return;
    }

    let mut close = false;
    let mut do_plan = false;
    let mut do_run = false;
    let mut pick_in = false;
    let mut pick_out = false;
    let mut st = app.harmonise_dialog.take().unwrap();

    let running = st.running.load(Ordering::Relaxed);
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_harmonise_dialog_v1");
    let window = egui::Window::new("octa_harmonise")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(660.0)
            .default_height(480.0)
            .min_width(480.0)
            .min_height(280.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("harmonise_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(t("harmonise.title")).strong().size(16.0),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
            });

        if minimized {
            return;
        }

        egui::Panel::bottom("harmonise_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let can_plan = !running && !st.folder.trim().is_empty();
                    let b = ui.add_enabled(can_plan, egui::Button::new(t("harmonise.plan")));
                    if b.clicked() {
                        do_plan = true;
                    }
                    b.on_hover_text(t("harmonise.plan_hint"));

                    // Run needs a plan AND an output folder. Both disabled
                    // reasons are spelled out rather than leaving a dead button.
                    let has_plan = st.plan.is_some();
                    let has_out = !st.out_dir.trim().is_empty();
                    let can_run = !running && has_plan && has_out;
                    let r = ui.add_enabled(can_run, egui::Button::new(t("harmonise.run")));
                    if r.clicked() {
                        do_run = true;
                    }
                    if can_run {
                        r.on_hover_text(t("harmonise.run_hint"));
                    } else if !has_plan {
                        r.on_disabled_hover_text(t("harmonise.need_plan"));
                    } else {
                        r.on_disabled_hover_text(t("harmonise.need_out_dir"));
                    }

                    if running {
                        ui.spinner();
                        let (done, total) = st.progress.lock().map(|p| *p).unwrap_or((0, 0));
                        if total > 0 {
                            ui.label(format!("{done}/{total}"));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("harmonise.body"));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(t("harmonise.folder"))
                    .on_hover_text(t("harmonise.folder_hint"));
                ui.add(egui::TextEdit::singleline(&mut st.folder).desired_width(360.0));
                if ui.button(t("dialog.swb_browse")).clicked() {
                    pick_in = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(t("harmonise.out_dir"))
                    .on_hover_text(t("harmonise.out_dir_hint"));
                ui.add(egui::TextEdit::singleline(&mut st.out_dir).desired_width(360.0));
                if ui.button(t("dialog.swb_browse")).clicked() {
                    pick_out = true;
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut st.recursive, t("harmonise.recursive"))
                    .on_hover_text(t("harmonise.recursive_hint"))
                    .changed()
                {
                    st.plan = None;
                }
                if ui
                    .checkbox(&mut st.ignore_case, t("harmonise.ignore_case"))
                    .on_hover_text(t("harmonise.ignore_case_hint"))
                    .changed()
                {
                    st.plan = None;
                }
                ui.checkbox(&mut st.overwrite, t("harmonise.overwrite"))
                    .on_hover_text(t("harmonise.overwrite_hint"));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match &st.plan {
                None => {
                    ui.weak(t("harmonise.not_planned"));
                }
                Some(plan) => {
                    egui::ScrollArea::vertical()
                        .id_salt("harmonise_plan")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(
                                t("harmonise.summary")
                                    .replace("{change}", &plan.to_change().to_string())
                                    .replace("{same}", &plan.already_matching().to_string())
                                    .replace("{refused}", &plan.refused().to_string()),
                            );

                            ui.add_space(6.0);
                            ui.label(RichText::new(t("harmonise.target")).strong());
                            for c in &plan.target {
                                ui.weak(format!("  {} ({})", c.name, c.data_type));
                            }

                            // The lossy part, shown before Run is pressed.
                            let dropped = plan.all_dropped();
                            if !dropped.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(t("harmonise.dropped"))
                                        .strong()
                                        .color(egui::Color32::from_rgb(0xE0, 0x9F, 0x5E)),
                                );
                                ui.label(t("harmonise.dropped_warn"));
                                for c in &dropped {
                                    ui.weak(format!("  {c}"));
                                }
                            }

                            let refused: Vec<&octa::data::harmonise::FileAction> = plan
                                .actions
                                .iter()
                                .filter(|a| {
                                    matches!(
                                        a.status,
                                        octa::data::harmonise::FileStatus::Refused(_)
                                    )
                                })
                                .collect();
                            if !refused.is_empty() {
                                ui.add_space(8.0);
                                ui.label(RichText::new(t("harmonise.refused")).strong());
                                ui.label(t("harmonise.refused_hint"));
                                for a in refused {
                                    if let octa::data::harmonise::FileStatus::Refused(why) =
                                        &a.status
                                    {
                                        ui.weak(format!(
                                            "  {}: {why}",
                                            a.input
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_default()
                                        ));
                                    }
                                }
                            }
                        });
                }
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if pick_in && let Some(d) = rfd::FileDialog::new().pick_folder() {
        st.folder = d.display().to_string();
        st.plan = None;
    }
    if pick_out && let Some(d) = rfd::FileDialog::new().pick_folder() {
        st.out_dir = d.display().to_string();
    }

    if do_plan {
        spawn_plan(&mut st, ctx);
    }
    if do_run {
        spawn_run(&mut st, ctx);
    }

    if !close {
        app.harmonise_dialog = Some(st);
    }
}

/// Scan the folder and build a plan, off the UI thread.
fn spawn_plan(st: &mut HarmoniseState, ctx: &egui::Context) {
    let dir = std::path::PathBuf::from(st.folder.trim());
    let out_dir = std::path::PathBuf::from(st.out_dir.trim());
    let recursive = st.recursive;
    let ignore_case = st.ignore_case;
    let overwrite = st.overwrite;

    st.running.store(true, Ordering::Relaxed);
    let running = std::sync::Arc::clone(&st.running);
    let slot = std::sync::Arc::clone(&st.plan_slot);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let outcome = if dir.is_dir() {
            let registry = octa::formats::FormatRegistry::new();
            let (files, _skipped) = collect_schemas(&dir, recursive, &registry);
            if files.is_empty() {
                Err(t("harmonise.empty"))
            } else {
                // Largest variant wins: the shape most files already have is
                // the one that needs the fewest rewritten.
                let opts = octa::data::schema_drift::DriftOptions {
                    ignore_case,
                    recursive,
                };
                let report = octa::data::schema_drift::analyse(&files, &opts);
                let target = report
                    .variants
                    .first()
                    .map(|v| v.columns.clone())
                    .unwrap_or_default();
                let hopts = HarmoniseOptions {
                    root: dir.clone(),
                    out_dir,
                    ignore_case,
                    overwrite,
                };
                Ok(plan_harmonise(&files, &target, &hopts))
            }
        } else {
            Err(t("harmonise.empty"))
        };
        if let Ok(mut s) = slot.lock() {
            *s = Some(outcome);
        }
        running.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}

/// Execute the plan, off the UI thread.
fn spawn_run(st: &mut HarmoniseState, ctx: &egui::Context) {
    let Some(plan) = st.plan.clone() else {
        return;
    };
    let opts = HarmoniseOptions {
        root: std::path::PathBuf::from(st.folder.trim()),
        out_dir: std::path::PathBuf::from(st.out_dir.trim()),
        ignore_case: st.ignore_case,
        overwrite: st.overwrite,
    };

    st.running.store(true, Ordering::Relaxed);
    let running = std::sync::Arc::clone(&st.running);
    let slot = std::sync::Arc::clone(&st.result);
    let progress = std::sync::Arc::clone(&st.progress);
    let ctx = ctx.clone();
    let write_opts = octa::formats::write_options::WriteOptions::default();

    std::thread::spawn(move || {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let report = run_harmonise(
            &plan,
            &opts,
            &|done, total| {
                if let Ok(mut p) = progress.lock() {
                    *p = (done, total);
                }
            },
            &cancel,
            &write_opts,
        );
        if let Ok(mut s) = slot.lock() {
            *s = Some(Ok(report));
        }
        running.store(false, Ordering::Relaxed);
        ctx.request_repaint();
    });
}

impl OctaApp {
    /// Poll both worker slots. Called once per frame from the update loop.
    pub(crate) fn drain_harmonise(&mut self) {
        let Some(st) = &mut self.harmonise_dialog else {
            return;
        };

        // A finished plan stays in the dialog: the user reads it, then runs.
        let planned = st.plan_slot.lock().ok().and_then(|mut s| s.take());
        if let Some(outcome) = planned {
            match outcome {
                Ok(plan) => st.plan = Some(plan),
                Err(reason) => {
                    self.status_message = Some((reason, std::time::Instant::now()));
                    return;
                }
            }
        }

        // A finished run opens the report and closes the dialog.
        let done = st.result.lock().ok().and_then(|mut s| s.take());
        let Some(outcome) = done else {
            return;
        };
        let report = match outcome {
            Ok(r) => r,
            Err(reason) => {
                self.status_message = Some((reason, std::time::Instant::now()));
                self.harmonise_dialog = None;
                return;
            }
        };

        let note = t("harmonise.done")
            .replace("{n}", &report.written.to_string())
            .replace("{refused}", &report.refused.to_string());

        let mut tab = super::super::state::TabState::new(self.settings.default_search_mode);
        tab.table = report_table(&report);
        tab.custom_tab_label = Some(t("harmonise.tab_label"));
        tab.filter_dirty = true;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.harmonise_dialog = None;
        self.status_message = Some((note, std::time::Instant::now()));
    }
}

/// Open prefilled with `dir` (the drift dialog's Harmonise button).
pub(crate) fn open_for_folder(app: &mut OctaApp, dir: &std::path::Path) {
    app.harmonise_dialog = Some(HarmoniseState::new(dir.display().to_string()));
}
