//! "Export tables to files" dialog: the tables of one database schema into a
//! folder, one file per table (`octa::db::export`). Opened from the Databases
//! sidebar (a schema's or a table's context menu) or its shortcut on an open
//! database tab. The table list comes from the sidebar's own listing cache;
//! the export runs on a worker with its own connection (it can take minutes,
//! and the shared one would block the sidebar meanwhile), reporting progress
//! per page and stopping between pages when asked.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::db::export::{ExportFormat, ExportRequest, TableOutcome};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};
use octa::ui::status_bar::format_number;

use super::super::db_browser::{DbListState, join_path};
use super::super::state::OctaApp;

/// Where the worker is: table index, table count, table name, rows so far.
type ProgressSlot = Arc<Mutex<Option<(usize, usize, String, usize)>>>;
type ResultSlot = Arc<Mutex<Option<Result<Vec<TableOutcome>, String>>>>;

struct ExportJob {
    /// How many tables were asked for, for the summary.
    requested: usize,
    progress: ProgressSlot,
    result: ResultSlot,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct DbExportState {
    size: DialogSize,
    conn_id: String,
    catalog: Option<String>,
    schema: String,
    /// "schema @ connection", for the header line.
    source_label: String,
    /// Ticked tables. `None` until the listing arrives, then every table (or
    /// just the one right-clicked).
    ticked: Option<HashSet<String>>,
    /// The table the dialog was opened from, ticked alone.
    preselect: Option<String>,
    find: String,
    dir: Option<PathBuf>,
    format: ExportFormat,
    overwrite: bool,
    job: Option<ExportJob>,
    /// The last run's summary: ok flag, message, and per-table failures.
    summary: Option<(bool, String, Vec<String>)>,
}

impl OctaApp {
    /// Entry from the sidebar: `table` is the right-clicked table, if any.
    pub(crate) fn open_db_export_dialog(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        catalog: Option<String>,
        schema: String,
        table: Option<String>,
    ) {
        let conn_name = self
            .find_db_conn(&conn_id)
            .map(|c| c.name)
            .unwrap_or_default();
        let qualified = match &catalog {
            Some(c) => format!("{c}.{schema}"),
            None => schema.clone(),
        };
        self.ensure_db_listing(ctx, conn_id.clone(), listing_key(&catalog, &schema));
        self.db_export_dialog = Some(DbExportState {
            size: DialogSize::Normal,
            source_label: format!("{qualified} @ {conn_name}"),
            conn_id,
            catalog,
            schema,
            ticked: None,
            preselect: table,
            find: String::new(),
            dir: None,
            format: ExportFormat::Parquet,
            overwrite: false,
            job: None,
            summary: None,
        });
    }

    /// The shortcut: export the schema of the open database table, which is
    /// the one place outside the sidebar that names a schema.
    pub(crate) fn open_db_export_for_active_tab(&mut self, ctx: &egui::Context) {
        let Some(origin) = self.tabs[self.active_tab].db_origin.clone() else {
            self.status_message = Some((t("dialog.dbx_need_schema"), std::time::Instant::now()));
            return;
        };
        self.open_db_export_dialog(
            ctx,
            origin.conn_id,
            origin.catalog,
            origin.schema,
            Some(origin.table),
        );
    }

    fn spawn_db_export(&self, st: &mut DbExportState, tables: Vec<String>, ctx: &egui::Context) {
        let (Some(conn), Some(dir)) = (self.find_db_conn(&st.conn_id), st.dir.clone()) else {
            return;
        };
        let req = ExportRequest {
            catalog: st.catalog.clone(),
            schema: st.schema.clone(),
            tables,
            dir,
            format: st.format,
            overwrite: st.overwrite,
            options: self.settings.write_options.clone(),
        };
        let job = ExportJob {
            requested: req.tables.len(),
            progress: Arc::new(Mutex::new(None)),
            result: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let (progress, result, cancel) =
            (job.progress.clone(), job.result.clone(), job.cancel.clone());
        st.job = Some(job);
        st.summary = None;
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        let task = self.long_tasks.start(octa::i18n::t("dialog.dbx_title"));
        std::thread::spawn(move || {
            let _task = task;
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
            let ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
            let outcome = octa::db::connect(&conn, secret.as_deref(), ssh_secret.as_deref())
                .and_then(|mut c| {
                    octa::db::export::export_tables(
                        c.as_mut(),
                        &req,
                        &|p| {
                            if let Ok(mut g) = progress.lock() {
                                *g = Some((p.index, p.count, p.table.to_string(), p.rows));
                            }
                            ctx.request_repaint();
                        },
                        &cancel,
                    )
                })
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = result.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }
}

/// The sidebar listing key of a schema node.
fn listing_key(catalog: &Option<String>, schema: &str) -> String {
    match catalog {
        Some(c) => join_path(&[c, schema]),
        None => join_path(&[schema]),
    }
}

/// The summary line and the failures of a finished run.
fn summarise(
    outcomes: &[TableOutcome],
    requested: usize,
    dir: &std::path::Path,
) -> (bool, String, Vec<String>) {
    let mut written = 0usize;
    let mut rows = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();
    for o in outcomes {
        match o {
            TableOutcome::Written { rows: n, .. } => {
                written += 1;
                rows += n;
            }
            TableOutcome::Skipped { .. } => skipped += 1,
            TableOutcome::Failed { table, error } => failures.push(format!("{table}: {error}")),
        }
    }
    let mut msg = t("dialog.dbx_done")
        .replace("{written}", &format_number(written))
        .replace("{total}", &format_number(requested))
        .replace("{rows}", &format_number(rows))
        .replace("{dir}", &dir.display().to_string());
    if skipped > 0 {
        msg.push(' ');
        msg.push_str(&t("dialog.dbx_skipped").replace("{n}", &format_number(skipped)));
    }
    if outcomes.len() < requested {
        msg.push(' ');
        msg.push_str(&t("dialog.dbx_stopped"));
    }
    if !failures.is_empty() {
        msg.push(' ');
        msg.push_str(&t("dialog.dbx_failed").replace("{n}", &format_number(failures.len())));
    }
    (failures.is_empty(), msg, failures)
}

pub(crate) fn render_db_export_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.db_export_dialog.take() else {
        return;
    };
    let mut close = false;
    let mut run: Option<Vec<String>> = None;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // The table list, from the sidebar's cache.
    let listing = app.db_browser.listings.lock().ok().and_then(|m| {
        m.get(&(st.conn_id.clone(), listing_key(&st.catalog, &st.schema)))
            .cloned()
    });
    let tables: Option<Result<Vec<String>, String>> = match listing {
        Some(DbListState::Tables(names)) => Some(Ok(names)),
        Some(DbListState::Error(e)) => Some(Err(e)),
        _ => None,
    };
    if st.ticked.is_none()
        && let Some(Ok(names)) = &tables
    {
        st.ticked = Some(match &st.preselect {
            Some(one) if names.contains(one) => HashSet::from([one.clone()]),
            _ => names.iter().cloned().collect(),
        });
    }

    // Drain a finished run into the summary.
    if let Some(job) = &st.job
        && let Some(res) = job.result.lock().ok().and_then(|mut g| g.take())
    {
        let requested = job.requested;
        st.summary = Some(match res {
            Ok(outcomes) => summarise(
                &outcomes,
                requested,
                st.dir.as_deref().unwrap_or(std::path::Path::new("")),
            ),
            Err(e) => (false, e, Vec::new()),
        });
        st.job = None;
    }
    let running = st.job.is_some();

    let dialog_id = egui::Id::new("octa_db_export_dialog");
    let window = egui::Window::new("octa_db_export")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(480.0)
            .default_height(460.0)
            .min_width(380.0)
            .min_height(300.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("db_export_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                octa::ui::control_row::control_line(ui, |ui| {
                    ui.label(RichText::new(t("dialog.dbx_title")).strong().size(16.0));
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

        egui::Panel::bottom("db_export_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                if let Some((ok, msg, failures)) = &st.summary {
                    draw_result_message(ui, *ok, msg);
                    for f in failures {
                        octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, f);
                    }
                    ui.add_space(4.0);
                }
                if let Some(job) = &st.job
                    && let Some((i, n, table, rows)) =
                        job.progress.lock().ok().and_then(|g| g.clone())
                {
                    ui.label(
                        t("dialog.dbx_running")
                            .replace("{i}", &(i + 1).to_string())
                            .replace("{n}", &n.to_string())
                            .replace("{table}", &table)
                            .replace("{rows}", &format_number(rows)),
                    );
                }
                octa::ui::control_row::control_line(ui, |ui| {
                    let ticked: Vec<String> = match (&tables, &st.ticked) {
                        // In listing order, not the set's.
                        (Some(Ok(names)), Some(set)) => {
                            names.iter().filter(|n| set.contains(*n)).cloned().collect()
                        }
                        _ => Vec::new(),
                    };
                    let ready = !running && !ticked.is_empty() && st.dir.is_some();
                    if ui
                        .add_enabled(ready, egui::Button::new(t("dialog.dbx_export")))
                        .on_hover_text(t("dialog.dbx_export_hint"))
                        .on_disabled_hover_text(t("dialog.dbx_export_off"))
                        .clicked()
                    {
                        run = Some(ticked);
                    }
                    if let Some(job) = &st.job {
                        ui.spinner();
                        if ui
                            .button(t("dialog.dbx_stop"))
                            .on_hover_text(t("dialog.dbx_stop_hint"))
                            .clicked()
                        {
                            job.cancel.store(true, Ordering::Relaxed);
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
            ui.label(RichText::new(t("dialog.dbx_intro")).small().weak());
            ui.add_space(6.0);
            ui.monospace(&st.source_label);
            ui.add_space(6.0);
            ui.add_enabled_ui(!running, |ui| {
                tables_section(ui, &mut st, tables.as_ref());
                ui.add_space(8.0);
                egui::Grid::new("db_export_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        octa::ui::control_row::pin_control_height(ui);
                        ui.label(t("dialog.dbx_folder"))
                            .on_hover_text(t("dialog.dbx_choose_hint"));
                        octa::ui::control_row::control_line(ui, |ui| {
                            if ui
                                .button(t("dialog.dbx_choose"))
                                .on_hover_text(t("dialog.dbx_choose_hint"))
                                .clicked()
                                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            {
                                st.dir = Some(dir);
                            }
                            match &st.dir {
                                Some(dir) => {
                                    ui.label(RichText::new(dir.display().to_string()).weak())
                                }
                                None => ui.label(RichText::new(t("dialog.dbx_no_folder")).weak()),
                            };
                        });
                        ui.end_row();

                        ui.label(t("dialog.dbx_format"))
                            .on_hover_text(t("dialog.dbx_format_hint"));
                        egui::ComboBox::from_id_salt("db_export_format")
                            .selected_text(st.format.label())
                            .show_ui(ui, |ui| {
                                for f in ExportFormat::ALL {
                                    ui.selectable_value(&mut st.format, f, f.label());
                                }
                            })
                            .response
                            .on_hover_text(t("dialog.dbx_format_hint"));
                        ui.end_row();
                    });
                ui.checkbox(&mut st.overwrite, t("dialog.dbx_overwrite"))
                    .on_hover_text(t("dialog.dbx_overwrite_hint"));
            });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if let Some(tables) = run {
        app.spawn_db_export(&mut st, tables, ctx);
    }
    if close {
        // Closing stops a run between pages; the file being written is left
        // out, finished ones stay.
        if let Some(job) = &st.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    } else {
        app.db_export_dialog = Some(st);
    }
}

/// The tick list: All / None, a find field, and one box per table.
fn tables_section(
    ui: &mut egui::Ui,
    st: &mut DbExportState,
    tables: Option<&Result<Vec<String>, String>>,
) {
    let names = match tables {
        None => {
            octa::ui::control_row::control_line(ui, |ui| {
                ui.spinner();
                ui.label(t("dialog.dbx_loading"));
            });
            return;
        }
        Some(Err(e)) => {
            octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            return;
        }
        Some(Ok(names)) => names,
    };
    let ticked = st.ticked.get_or_insert_with(HashSet::new);
    octa::ui::control_row::control_line(ui, |ui| {
        ui.label(RichText::new(t("dialog.dbx_tables")).strong());
        if ui
            .button(t("dialog.dbx_all"))
            .on_hover_text(t("dialog.dbx_all_hint"))
            .clicked()
        {
            ticked.extend(names.iter().cloned());
        }
        if ui
            .button(t("dialog.dbx_none"))
            .on_hover_text(t("dialog.dbx_none_hint"))
            .clicked()
        {
            ticked.clear();
        }
        ui.add(
            egui::TextEdit::singleline(&mut st.find)
                .hint_text(t("dialog.dbx_find"))
                .desired_width(140.0),
        )
        .on_hover_text(t("dialog.dbx_find_hint"));
        ui.label(
            RichText::new(
                t("dialog.dbx_count")
                    .replace("{n}", &format_number(ticked.len()))
                    .replace("{total}", &format_number(names.len())),
            )
            .weak(),
        );
    });
    let needle = st.find.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("db_export_tables")
        .max_height(200.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for name in names {
                if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                    continue;
                }
                let mut on = ticked.contains(name);
                if ui
                    .checkbox(&mut on, name)
                    .on_hover_text(t("dialog.dbx_table_hint"))
                    .changed()
                {
                    if on {
                        ticked.insert(name.clone());
                    } else {
                        ticked.remove(name);
                    }
                }
            }
        });
}
