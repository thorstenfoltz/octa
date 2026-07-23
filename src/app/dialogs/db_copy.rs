//! "Copy table to another connection" dialog: server-to-server copy through
//! DuckDB (`octa::db::copy`). Opened from the sidebar Databases tree's table
//! context menu. The copy runs on a worker thread (token auth can shell out
//! to a cloud CLI, and the copy itself is a network job); the dialog polls
//! the result slot per frame, like the Settings "Test connection" button.

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::RichText;

use octa::db::copy::{CopyLane, DbCopyEnd, DbCopyReport, choose_lane, copy_table};
use octa::db::{DbEngine, DbWriteMode};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

/// Shared slot the copy worker writes its outcome into.
type CopySlot = Arc<Mutex<Option<Result<DbCopyReport, String>>>>;

pub(crate) struct DbCopyState {
    pub(crate) size: DialogSize,
    /// Fixed source (the right-clicked table).
    pub(crate) src_conn_id: String,
    pub(crate) src_label: String,
    /// Catalog for a three-level source engine, else None.
    pub(crate) src_catalog: Option<String>,
    pub(crate) src_schema: String,
    pub(crate) src_table: String,
    /// Target picks.
    pub(crate) tgt_conn_id: String,
    pub(crate) tgt_schema: String,
    pub(crate) tgt_table: String,
    pub(crate) mode: DbWriteMode,
    /// In-flight worker slot; `Some` while a copy runs.
    pub(crate) job: Option<CopySlot>,
    /// Last finished outcome (ok flag + message).
    pub(crate) result_msg: Option<(bool, String)>,
}

/// The sensible default schema on a target engine: Postgres tables usually
/// live in `public`; a MySQL "schema" is a database, so the connection's own.
fn default_schema(engine: DbEngine, database: &str) -> String {
    match engine {
        DbEngine::Postgres => "public".to_string(),
        _ => database.to_string(),
    }
}

impl OctaApp {
    /// Entry from the sidebar tree's context menu.
    pub(crate) fn open_db_copy_dialog(
        &mut self,
        conn_id: String,
        catalog: Option<String>,
        schema: String,
        table: String,
    ) {
        let src_name = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == conn_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let src_qual = match &catalog {
            Some(c) => format!("{c}.{schema}.{table}"),
            None => format!("{schema}.{table}"),
        };
        // Pre-pick the first target that isn't the source (any engine: the
        // universal lane copies to anything).
        let tgt = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id != conn_id);
        self.db_copy_dialog = Some(DbCopyState {
            size: DialogSize::Normal,
            src_label: format!("{src_qual} @ {src_name}"),
            src_conn_id: conn_id,
            src_catalog: catalog,
            src_schema: schema,
            src_table: table.clone(),
            tgt_conn_id: tgt.map(|c| c.id.clone()).unwrap_or_default(),
            tgt_schema: tgt
                .map(|c| default_schema(c.engine, &c.database))
                .unwrap_or_default(),
            tgt_table: table,
            mode: DbWriteMode::Create,
            job: None,
            result_msg: None,
        });
    }

    fn spawn_db_copy(&self, st: &mut DbCopyState, ctx: &egui::Context) {
        let (Some(src_conn), Some(tgt_conn)) = (
            self.settings
                .db_connections
                .iter()
                .find(|c| c.id == st.src_conn_id)
                .cloned(),
            self.settings
                .db_connections
                .iter()
                .find(|c| c.id == st.tgt_conn_id)
                .cloned(),
        ) else {
            st.result_msg = Some((false, t("dialog.dbc_need_target")));
            return;
        };
        let source = DbCopyEnd {
            conn: src_conn,
            catalog: st.src_catalog.clone(),
            schema: st.src_schema.clone(),
            table: st.src_table.clone(),
        };
        let target = DbCopyEnd {
            conn: tgt_conn,
            // Targets are addressed by the dialog's schema/table; catalog
            // engines are never fast-lane targets and write to their default
            // catalog on the universal lane.
            catalog: None,
            schema: st.tgt_schema.trim().to_string(),
            table: st.tgt_table.trim().to_string(),
        };
        if target.schema.is_empty() || target.table.is_empty() {
            st.result_msg = Some((false, t("dialog.dbc_need_target")));
            return;
        }
        let slot: CopySlot = Arc::new(Mutex::new(None));
        st.job = Some(slot.clone());
        st.result_msg = None;
        let mode = st.mode;
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let src_secret =
                octa::ui::settings::db_secrets::get_db_secret(&source.conn.id, &settings);
            let tgt_secret =
                octa::ui::settings::db_secrets::get_db_secret(&target.conn.id, &settings);
            let outcome = copy_table(
                &source,
                src_secret.as_deref(),
                &target,
                tgt_secret.as_deref(),
                mode,
            )
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }
}

pub(crate) fn render_db_copy_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.db_copy_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut run = false;
    let mut st = app.db_copy_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // Drain a finished worker into the result line.
    if let Some(slot) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.result_msg = Some(match res {
            Ok(report) => (
                true,
                t("dialog.dbc_done").replace("{n}", &report.rows_copied.to_string()),
            ),
            Err(e) => (false, e),
        });
        st.job = None;
    }
    let running = st.job.is_some();

    let dialog_id = egui::Id::new("octa_db_copy_dialog");
    let window = egui::Window::new("octa_db_copy")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(280.0)
            .min_width(360.0)
            .min_height(200.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("db_copy_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("dialog.dbc_title")).strong().size(16.0));
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
        egui::Panel::bottom("db_copy_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!running, egui::Button::new(t("dialog.dbc_copy")))
                        .clicked()
                    {
                        run = true;
                    }
                    if running {
                        ui.spinner();
                        ui.label(t("dialog.dbc_running"));
                    } else if let Some((ok, msg)) = &st.result_msg {
                        let color = if *ok {
                            egui::Color32::from_rgb(0x30, 0x80, 0x30)
                        } else {
                            ui.visuals().error_fg_color
                        };
                        ui.colored_label(color, msg);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(
                RichText::new(t("dialog.dbc_intro"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);
            egui::Grid::new("db_copy_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("dialog.dbc_source"));
                    ui.monospace(&st.src_label);
                    ui.end_row();

                    ui.label(t("dialog.dbc_target"));
                    let selected = app
                        .settings
                        .db_connections
                        .iter()
                        .find(|c| c.id == st.tgt_conn_id)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| t("dialog.dbc_pick"));
                    egui::ComboBox::from_id_salt("db_copy_target")
                        .selected_text(selected)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            // Any connection may be a target: the universal lane
                            // copies to any engine.
                            for c in app.settings.db_connections.iter() {
                                let mut label = format!("{} ({})", c.name, c.engine.label());
                                if !c.allow_writes {
                                    label.push_str(&format!("  [{}]", t("db.copy_writes_off")));
                                }
                                if ui.selectable_label(st.tgt_conn_id == c.id, label).clicked() {
                                    st.tgt_conn_id = c.id.clone();
                                    st.tgt_schema = default_schema(c.engine, &c.database);
                                }
                            }
                        });
                    ui.end_row();

                    // Show which lane the current source->target pair will use.
                    let src_engine = app
                        .settings
                        .db_connections
                        .iter()
                        .find(|c| c.id == st.src_conn_id)
                        .map(|c| c.engine);
                    let tgt_engine = app
                        .settings
                        .db_connections
                        .iter()
                        .find(|c| c.id == st.tgt_conn_id)
                        .map(|c| c.engine);
                    if let (Some(s), Some(g)) = (src_engine, tgt_engine) {
                        ui.label(t("dialog.dbc_lane"));
                        ui.label(t(lane_note_key(choose_lane(s, g))));
                        ui.end_row();
                    }

                    ui.label(t("dialog.dbc_schema"));
                    ui.add(egui::TextEdit::singleline(&mut st.tgt_schema).desired_width(220.0));
                    ui.end_row();

                    ui.label(t("dialog.dbc_table"));
                    ui.add(egui::TextEdit::singleline(&mut st.tgt_table).desired_width(220.0));
                    ui.end_row();

                    ui.label(t("dialog.swb_mode"));
                    egui::ComboBox::from_id_salt("db_copy_mode")
                        .selected_text(mode_label(st.mode))
                        .show_ui(ui, |ui| {
                            for mode in [
                                DbWriteMode::Create,
                                DbWriteMode::Append,
                                DbWriteMode::Replace,
                            ] {
                                ui.selectable_value(&mut st.mode, mode, mode_label(mode));
                            }
                        });
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label(
                RichText::new(t("dialog.dbc_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        app.spawn_db_copy(&mut st, ctx);
    }
    // Closing mid-copy is allowed: the worker finishes (or fails) server-side
    // on its own; only the result line is discarded with the dialog.
    if !close {
        app.db_copy_dialog = Some(st);
    }
}

fn mode_label(mode: DbWriteMode) -> String {
    match mode {
        DbWriteMode::Create => t("dialog.swb_mode_create"),
        DbWriteMode::Append => t("dialog.swb_mode_append"),
        DbWriteMode::Replace => t("dialog.swb_mode_replace"),
    }
}

/// i18n key describing a copy lane (fast server-to-server vs streamed).
fn lane_note_key(lane: CopyLane) -> &'static str {
    match lane {
        CopyLane::Fast => "db.copy_lane_fast",
        CopyLane::Universal => "db.copy_lane_universal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_note_matches_lane() {
        assert_eq!(
            lane_note_key(choose_lane(DbEngine::Postgres, DbEngine::MySql)),
            "db.copy_lane_fast"
        );
        assert_eq!(
            lane_note_key(choose_lane(DbEngine::Postgres, DbEngine::Snowflake)),
            "db.copy_lane_universal"
        );
    }
}
