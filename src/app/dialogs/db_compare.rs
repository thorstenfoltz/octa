//! "Compare with database or cloud" dialog: diffs the active tab against a
//! table on a saved connection, or against an object in cloud storage.
//!
//! No new comparison logic: the database side is read by
//! `octa::db::fetch_table`, the cloud side is downloaded by
//! `octa::cloud::fetch_url_to_temp` and read through the normal registry, and
//! both go into `compare::compare_join`, the same engine behind file-vs-file
//! `--diff --diff-mode join`. The read runs on a worker thread (it opens a
//! network connection and may shell out to a cloud CLI for a token), and the
//! dialog polls the result slot per frame, exactly like the copy dialog next
//! door.

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::data::DataTable;
use octa::db::DbEngine;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;
use super::widgets::multi_col_picker;

/// What the worker hands back: the ready-made compare table plus whether
/// either side sat on the row cap.
pub(crate) struct CompareOutcome {
    pub(crate) table: DataTable,
    pub(crate) capped: bool,
    pub(crate) db_label: String,
}

type CompareSlot = Arc<Mutex<Option<Result<CompareOutcome, String>>>>;

/// Where the B side of the comparison comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompareSource {
    Database,
    Cloud,
}

/// The resolved B-side target, moved into the worker thread. Built on the UI
/// thread so a missing table name or URL reports without spawning anything.
enum SideB {
    Db {
        conn: Box<octa::db::DbConnection>,
        catalog: Option<String>,
        schema: String,
        table: String,
    },
    Cloud(String),
}

pub(crate) struct DbCompareState {
    pub(crate) size: DialogSize,
    /// Connection to read the B side from.
    pub(crate) conn_id: String,
    pub(crate) catalog: String,
    pub(crate) schema: String,
    pub(crate) table: String,
    /// Which kind of target the B side is.
    pub(crate) source: CompareSource,
    /// Cloud object URL, used when `source` is `Cloud`.
    pub(crate) url: String,
    /// Key columns, indices into the active tab's column list.
    pub(crate) keys: Vec<usize>,
    /// In-flight worker slot; `Some` while a comparison runs.
    pub(crate) job: Option<CompareSlot>,
    pub(crate) result_msg: Option<(bool, String)>,
}

/// The sensible default schema per engine: Postgres tables usually live in
/// `public`, while a MySQL "schema" is the database itself.
fn default_schema(engine: DbEngine, database: &str) -> String {
    match engine {
        DbEngine::Postgres | DbEngine::Redshift => "public".to_string(),
        _ => database.to_string(),
    }
}

impl OctaApp {
    /// Entry from the Analyse menu and from the db tree's table context menu.
    /// `prefill` carries (catalog, schema, table) when the tree opened it.
    pub(crate) fn open_db_compare_dialog(
        &mut self,
        conn_id: Option<String>,
        prefill: Option<(Option<String>, String, String)>,
    ) {
        if self.settings.db_connections.is_empty() && self.settings.cloud_connections.is_empty() {
            self.status_message = Some((t("dbcmp.no_connections"), std::time::Instant::now()));
            return;
        }
        if self
            .tabs
            .get(self.active_tab)
            .is_none_or(|tab| tab.table.col_count() == 0)
        {
            self.status_message = Some((t("dbcmp.need_table"), std::time::Instant::now()));
            return;
        }
        let conn = conn_id
            .and_then(|id| self.settings.db_connections.iter().find(|c| c.id == id))
            .or_else(|| self.settings.db_connections.first());
        // A cloud-only user has no database connections. Open on the cloud
        // side rather than refusing.
        let (sel_id, def_schema) = match conn {
            Some(c) => (c.id.clone(), default_schema(c.engine, &c.database)),
            None => (String::new(), String::new()),
        };
        let source = if sel_id.is_empty() {
            CompareSource::Cloud
        } else {
            CompareSource::Database
        };
        let (catalog, schema, table) = match prefill {
            Some((cat, sch, tbl)) => (cat.unwrap_or_default(), sch, tbl),
            None => (String::new(), def_schema, String::new()),
        };
        self.db_compare_dialog = Some(DbCompareState {
            size: DialogSize::Normal,
            conn_id: sel_id,
            catalog,
            schema,
            table,
            source,
            url: String::new(),
            keys: Vec::new(),
            job: None,
            result_msg: None,
        });
    }

    fn spawn_db_compare(&self, st: &mut DbCompareState, ctx: &egui::Context) {
        // Resolve the B side on the UI thread: a missing target must report
        // in the dialog, not inside a worker.
        let (side, db_label) = match st.source {
            CompareSource::Database => {
                let Some(conn) = self
                    .settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == st.conn_id)
                    .cloned()
                else {
                    st.result_msg = Some((false, t("dbcmp.no_connections")));
                    return;
                };
                if st.table.trim().is_empty() {
                    st.result_msg = Some((false, t("dbcmp.need_target")));
                    return;
                }
                let schema = st.schema.trim().to_string();
                let table = st.table.trim().to_string();
                let label = format!("{schema}.{table} @ {}", conn.name);
                (
                    SideB::Db {
                        conn: Box::new(conn),
                        catalog: (!st.catalog.trim().is_empty())
                            .then(|| st.catalog.trim().to_string()),
                        schema,
                        table,
                    },
                    label,
                )
            }
            CompareSource::Cloud => {
                let url = st.url.trim().to_string();
                if url.is_empty() {
                    st.result_msg = Some((false, t("dbcmp.need_url")));
                    return;
                }
                (SideB::Cloud(url.clone()), url)
            }
        };

        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        // Snapshot with edits applied: the comparison must reflect what the
        // user sees, not what was last saved.
        let mut left = tab.table.clone();
        left.apply_edits();
        let key_names: Vec<String> = st
            .keys
            .iter()
            .filter_map(|i| left.columns.get(*i).map(|c| c.name.clone()))
            .collect();
        if key_names.is_empty() {
            st.result_msg = Some((false, t("dbcmp.need_keys")));
            return;
        }

        let slot: CompareSlot = Arc::new(Mutex::new(None));
        st.job = Some(slot.clone());
        st.result_msg = None;
        let settings = self.settings.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<CompareOutcome> {
                let right = match side {
                    SideB::Db {
                        conn,
                        catalog,
                        schema,
                        table,
                    } => {
                        let secret =
                            octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                        octa::db::fetch_table::fetch_table(
                            &conn,
                            secret.as_deref(),
                            catalog.as_deref(),
                            &schema,
                            &table,
                        )?
                    }
                    // A cloud object is just a file: download it, then read it
                    // through the same registry every other read uses.
                    SideB::Cloud(url) => {
                        let tmp = octa::cloud::fetch_url_to_temp(&url, &settings)?;
                        let cap = if settings.max_decompressed_unlimited {
                            u64::MAX
                        } else {
                            settings.max_decompressed_bytes
                        };
                        octa::formats::read_table_auto(&tmp, None, cap)?
                    }
                };
                let cap = octa::formats::initial_load_rows();
                let capped = right.row_count() >= cap || left.row_count() >= cap;
                let result = octa::data::compare::compare_join(&left, &right, &key_names)?;
                Ok(CompareOutcome {
                    table: octa::data::compare::build_compare_table(&left, &right, &result),
                    capped,
                    db_label,
                })
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Open a finished comparison in a detached tab, the same way Summary and
    /// File internals do.
    fn open_compare_result_tab(&mut self, outcome: CompareOutcome) {
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::super::state::TabState::new(default_search_mode);
        new_tab.table = outcome.table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!("{} - {}", t("dbcmp.tab_label"), outcome.db_label));
        if outcome.capped {
            new_tab.parse_error_banner = Some(t("dbcmp.capped_note"));
        }
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }
}

pub(crate) fn render_db_compare_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.db_compare_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut st = app.db_compare_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // Drain a finished worker: success opens a tab and closes the dialog,
    // failure stays put with the message so the user can fix the table name.
    let mut finished: Option<CompareOutcome> = None;
    if let Some(slot) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.job = None;
        match res {
            Ok(outcome) => finished = Some(outcome),
            Err(e) => st.result_msg = Some((false, e)),
        }
    }
    let running = st.job.is_some();

    let columns: Vec<String> = app
        .tabs
        .get(app.active_tab)
        .map(|tab| tab.table.columns.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();
    let connections: Vec<(String, String)> = app
        .settings
        .db_connections
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();

    let dialog_id = egui::Id::new("octa_db_compare_dialog");
    let window = egui::Window::new("octa_db_compare")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(340.0)
            .min_width(380.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("db_compare_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("dbcmp.title")).strong().size(16.0));
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

        egui::Panel::bottom("db_compare_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label(t("dbcmp.running"));
                    } else {
                        let target_named = match st.source {
                            CompareSource::Database => !st.table.trim().is_empty(),
                            CompareSource::Cloud => !st.url.trim().is_empty(),
                        };
                        let ready = target_named && !st.keys.is_empty();
                        let btn = ui.add_enabled(ready, egui::Button::new(t("dbcmp.run")));
                        if btn.clicked() {
                            run = true;
                        }
                        if !ready {
                            // Say which of the two things is missing rather
                            // than leaving a dead button with no explanation.
                            let why = if !target_named {
                                match st.source {
                                    CompareSource::Database => t("dbcmp.need_target"),
                                    CompareSource::Cloud => t("dbcmp.need_url"),
                                }
                            } else {
                                t("dbcmp.need_keys")
                            };
                            btn.on_disabled_hover_text(why.clone());
                            ui.label(
                                RichText::new(why)
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("dbcmp.body"));
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(t("dbcmp.source"));
                ui.radio_value(
                    &mut st.source,
                    CompareSource::Database,
                    t("dbcmp.source_db"),
                );
                ui.radio_value(
                    &mut st.source,
                    CompareSource::Cloud,
                    t("dbcmp.source_cloud"),
                );
            });
            ui.add_space(6.0);

            if st.source == CompareSource::Database {
                egui::Grid::new("db_compare_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(t("dbcmp.connection"))
                            .on_hover_text(t("dbcmp.connection_hint"));
                        let selected = connections
                            .iter()
                            .find(|(id, _)| *id == st.conn_id)
                            .map(|(_, name)| name.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("db_compare_conn")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                for (id, name) in &connections {
                                    ui.selectable_value(&mut st.conn_id, id.clone(), name);
                                }
                            })
                            .response
                            .on_hover_text(t("dbcmp.connection_hint"));
                        ui.end_row();

                        ui.label(t("dbcmp.catalog"))
                            .on_hover_text(t("dbcmp.catalog_hint"));
                        ui.add(
                            egui::TextEdit::singleline(&mut st.catalog)
                                .desired_width(220.0)
                                .hint_text(t("dbcmp.catalog_optional")),
                        )
                        .on_hover_text(t("dbcmp.catalog_hint"));
                        ui.end_row();

                        ui.label(t("dbcmp.schema"))
                            .on_hover_text(t("dbcmp.schema_hint"));
                        ui.add(egui::TextEdit::singleline(&mut st.schema).desired_width(220.0))
                            .on_hover_text(t("dbcmp.schema_hint"));
                        ui.end_row();

                        ui.label(t("dbcmp.table"))
                            .on_hover_text(t("dbcmp.table_hint"));
                        ui.add(egui::TextEdit::singleline(&mut st.table).desired_width(220.0))
                            .on_hover_text(t("dbcmp.table_hint"));
                        ui.end_row();
                    });
            } else {
                egui::Grid::new("db_compare_cloud_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(t("dbcmp.url")).on_hover_text(t("dbcmp.url_hint"));
                        ui.add(
                            egui::TextEdit::singleline(&mut st.url)
                                .desired_width(300.0)
                                .hint_text("s3://bucket/exports/day.parquet"),
                        )
                        .on_hover_text(t("dbcmp.url_hint"));
                        ui.end_row();
                    });
            }

            ui.add_space(8.0);
            ui.label(RichText::new(t("dbcmp.keys")).strong())
                .on_hover_text(t("dbcmp.keys_hint"));
            multi_col_picker(ui, "db_compare_keys", &mut st.keys, &columns);

            if let Some((ok, msg)) = &st.result_msg {
                ui.add_space(8.0);
                draw_result_message(ui, *ok, msg);
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        app.spawn_db_compare(&mut st, ctx);
    }

    if let Some(outcome) = finished {
        app.open_compare_result_tab(outcome);
        return; // dialog closes; the result is the tab
    }
    if !close {
        app.db_compare_dialog = Some(st);
    }
}
