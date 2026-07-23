//! Confirmed write-back of an edited db-origin tab to its live server table.
//!
//! Flow: Save (Ctrl+S) on a modified db-origin tab calls
//! [`OctaApp::begin_db_write_back`], which diffs the edits into a
//! `DbWriteBackPlan` and raises a forced-choice confirmation modal (modelled
//! on `schema_change_save.rs`). Confirm spawns a worker thread (connecting
//! can shell out to a cloud CLI, so it must never run on the UI thread) that
//! applies the plan in one transaction; `drain_db_write_back_job` picks up
//! the result per frame and re-baselines the tab on success.

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::db::write_back::{DbWriteBackPlan, DbWriteBackReport, apply_write_back};
use octa::i18n::t;

use super::super::state::{OctaApp, TabState};

/// A pending confirmation: everything the worker will need, captured at
/// Save time so a tab/settings change between Save and Confirm cannot
/// redirect the write.
pub(crate) struct DbWriteBackPrompt {
    pub(crate) tab_idx: usize,
    pub(crate) conn_id: String,
    pub(crate) schema: String,
    pub(crate) table: String,
    /// Quoted-for-humans target, e.g. `public.orders @ prod`.
    pub(crate) target_label: String,
    pub(crate) plan: DbWriteBackPlan,
    /// Column snapshot matching the plan's row layout.
    pub(crate) columns: Vec<octa::data::ColumnInfo>,
    pub(crate) pk_cols: Vec<String>,
}

/// One in-flight write-back (at most one at a time, app-wide).
pub(crate) struct DbWriteBackJob {
    pub(crate) tab_idx: usize,
    pub(crate) target_label: String,
    pub(crate) result: Arc<Mutex<Option<Result<DbWriteBackReport, String>>>>,
}

impl OctaApp {
    /// Entry point from the Save paths: diff the tab's edits and raise the
    /// confirmation modal (or a status message when there is nothing to do
    /// or the write cannot proceed).
    pub(crate) fn begin_db_write_back(&mut self, tab_idx: usize) {
        let status = |msg: String| (msg, std::time::Instant::now());
        if self.db_write_back_job.is_some() {
            self.status_message = Some(status(t("dialog.dwb_busy")));
            return;
        }
        let Some(origin) = self.tabs[tab_idx].db_origin.clone() else {
            return;
        };
        if !self.tabs[tab_idx].is_modified() {
            self.status_message = Some(status(t("db.wb_no_changes")));
            return;
        }
        // Belt and braces: is_readonly() already prevented edits on a
        // non-writable tab, but the connection may have changed since.
        if !self.db_origin_writable(&origin) {
            self.status_message = Some(status(t("db.tab_readonly_note")));
            return;
        }
        let conn_name = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == origin.conn_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();

        let mut snapshot = self.tabs[tab_idx].table.clone();
        snapshot.apply_edits();
        let plan = match octa::db::write_back::build_write_back_plan(&snapshot, &origin.pk_cols) {
            Ok(p) => p,
            Err(e) => {
                self.status_message = Some(status(format!("{} {e:#}", t("db.wb_failed"))));
                return;
            }
        };
        if plan.is_empty() {
            self.status_message = Some(status(t("db.wb_no_changes")));
            return;
        }
        self.pending_db_write_back = Some(DbWriteBackPrompt {
            tab_idx,
            conn_id: origin.conn_id,
            schema: origin.schema.clone(),
            table: origin.table.clone(),
            target_label: format!("{}.{} @ {conn_name}", origin.schema, origin.table),
            plan,
            columns: snapshot.columns.clone(),
            pk_cols: origin.pk_cols,
        });
    }

    /// Spawn the worker that applies a confirmed plan. Runs off the UI
    /// thread: secret resolution can shell out to the aws/az/gcloud CLIs.
    fn spawn_db_write_back(&mut self, prompt: DbWriteBackPrompt, ctx: &egui::Context) {
        let Some(conn) = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == prompt.conn_id)
            .cloned()
        else {
            self.status_message = Some((
                format!("{} connection no longer exists", t("db.wb_failed")),
                std::time::Instant::now(),
            ));
            return;
        };
        let result: Arc<Mutex<Option<Result<DbWriteBackReport, String>>>> =
            Arc::new(Mutex::new(None));
        self.db_write_back_job = Some(DbWriteBackJob {
            tab_idx: prompt.tab_idx,
            target_label: prompt.target_label,
            result: result.clone(),
        });
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<DbWriteBackReport> {
                octa::db::ensure_write_allowed(&conn, None)?;
                let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                // Safe to retry on a stale cached connection: apply_write_back
                // is one rolled-back-on-error transaction.
                cache.with_conn(&conn, secret.as_deref(), |c| {
                    apply_write_back(
                        c,
                        conn.engine,
                        &prompt.schema,
                        &prompt.table,
                        &prompt.columns,
                        &prompt.pk_cols,
                        &prompt.plan,
                    )
                })
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut slot) = result.lock() {
                *slot = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Per-frame drain of a finished write-back (update loop, next to
    /// `drain_sql_server_job`). On success the current rows ARE the server
    /// state: re-baseline the tab and clear its modified flag.
    pub(crate) fn drain_db_write_back_job(&mut self) {
        let Some(job) = &self.db_write_back_job else {
            return;
        };
        let outcome = match job.result.lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => Some(Err("write-back worker panicked".to_string())),
        };
        let Some(outcome) = outcome else {
            return;
        };
        let (tab_idx, target_label) = {
            let job = self.db_write_back_job.take().expect("job checked above");
            (job.tab_idx, job.target_label)
        };
        match outcome {
            Ok(report) => {
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    tab.table.apply_edits();
                    retag_db_meta(tab);
                    tab.table.clear_modified();
                }
                let n = report.deleted + report.updated + report.inserted;
                self.status_message = Some((
                    t("db.wb_done")
                        .replace("{n}", &n.to_string())
                        .replace("{target}", &target_label),
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                // Keep the edits (tab stays modified); the user can fix and
                // retry.
                self.status_message = Some((
                    format!("{} {e}", t("db.wb_failed")),
                    std::time::Instant::now(),
                ));
            }
        }
    }
}

/// After a successful write-back the tab's current rows are the server
/// state: tag every row `0..n` and snapshot rows + columns as the new
/// baseline. (`resync_db_meta_baseline` is close, but it keeps `None` tags,
/// which must become real tags here.)
fn retag_db_meta(tab: &mut TabState) {
    let Some(origin) = tab.db_origin.as_ref() else {
        return;
    };
    let original: std::collections::HashMap<i64, Vec<octa::data::CellValue>> = tab
        .table
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (i as i64, r.clone()))
        .collect();
    tab.table.db_meta = Some(octa::data::DbRowMeta {
        table_name: origin.table.clone(),
        schema: Some(origin.schema.clone()),
        row_tags: (0..tab.table.rows.len()).map(|i| Some(i as i64)).collect(),
        original,
        original_columns: tab.table.columns.iter().map(|c| c.name.clone()).collect(),
    });
}

/// The forced-choice confirmation modal (no close 'x'; Confirm / Cancel
/// only, like the schema-change prompt).
pub(crate) fn render_db_write_back_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = &app.pending_db_write_back else {
        return;
    };
    let mut proceed = false;
    let mut cancel = false;
    egui::Window::new(t("dialog.dwb_title"))
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(t("dialog.dwb_intro").replace("{target}", &prompt.target_label));
            ui.add_space(4.0);
            if !prompt.plan.updates.is_empty() {
                ui.monospace(format!(
                    "{} {}",
                    prompt.plan.updates.len(),
                    t("dialog.dwb_updates")
                ));
            }
            if !prompt.plan.inserts.is_empty() {
                ui.monospace(format!(
                    "{} {}",
                    prompt.plan.inserts.len(),
                    t("dialog.dwb_inserts")
                ));
            }
            if !prompt.plan.deletes.is_empty() {
                ui.monospace(format!(
                    "{} {}",
                    prompt.plan.deletes.len(),
                    t("dialog.dwb_deletes")
                ));
            }
            if !prompt.plan.added_columns.is_empty() {
                let names: Vec<&str> = prompt
                    .plan
                    .added_columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect();
                ui.monospace(format!("{} {}", t("dialog.dwb_add_cols"), names.join(", ")));
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(t("dialog.dwb_warn")).color(ui.visuals().warn_fg_color));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(t("dialog.dwb_proceed")).clicked() {
                    proceed = true;
                }
                if ui.button(t("common.cancel")).clicked() {
                    cancel = true;
                }
            });
        });

    if proceed {
        let prompt = app.pending_db_write_back.take().expect("prompt present");
        app.spawn_db_write_back(prompt, ctx);
    } else if cancel {
        app.pending_db_write_back = None;
        app.status_message = Some((t("dialog.dwb_cancelled"), std::time::Instant::now()));
    }
}
