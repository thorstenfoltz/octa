//! Analyses on live databases: which tab goes to the server, the worker, the
//! result note and the error window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use octa::data::DataTable;
use octa::data::correlation::{CorrMatrix, CorrMethod};
use octa::data::quality::QualityReport;
use octa::data::value_frequency::{BinningMode, ValueFrequency};
use octa::db::DbConnection;
use octa::db::pushdown::{LocalParts, ServerSource};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

use super::state::{DbOrigin, OctaApp, TabState};

/// The one place that decides whether an analysis runs on the server.
///
/// `Some` only for a live-database tab (it has a `db_origin`) that does not
/// hold every row (`source_has_more`, not `is_partial`, which reads a db tab
/// as complete), with the setting on and its connection still saved. A file,
/// cloud, API or SQL-result tab has no `db_origin`, so it always gets `None`
/// and keeps today's path: that is the spec's non-goal, pinned below.
pub(crate) fn server_source_for(
    tab: &TabState,
    pushdown_on: bool,
    conns: &[DbConnection],
) -> Option<ServerSource> {
    server_sources_for(&[tab], pushdown_on, conns)?.pop()
}

/// The multi-table sibling of [`server_source_for`], for the key analyses:
/// one source per tab, in order, only when every tab is a live-database
/// table on ONE saved connection (a statement cannot join across servers),
/// the setting is on, and at least one tab does not hold every row. A
/// complete database tab goes along, so both sides are counted alike.
pub(crate) fn server_sources_for(
    tabs: &[&TabState],
    pushdown_on: bool,
    conns: &[DbConnection],
) -> Option<Vec<ServerSource>> {
    if !pushdown_on || !tabs.iter().any(|t| t.source_has_more()) {
        return None;
    }
    let conn_id = &tabs.first()?.db_origin.as_ref()?.conn_id;
    let conn = conns.iter().find(|c| &c.id == conn_id)?;
    tabs.iter()
        .map(|t| {
            let o = t.db_origin.as_ref().filter(|o| &o.conn_id == conn_id)?;
            Some(ServerSource {
                conn: conn.clone(),
                catalog: o.catalog.clone(),
                schema: o.schema.clone(),
                table: o.table.clone(),
                filter: t
                    .server_view
                    .as_ref()
                    .and_then(|v| v.where_sql(conn.engine)),
                derived: t
                    .server_view
                    .as_ref()
                    .map(|v| v.derived.clone())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// The setting is on and some database table is partial, yet the analysis
/// cannot go to the server: the dialog says why its numbers cover loaded rows
/// only. A set of file tabs, even a capped one, never says it: no database is
/// involved.
pub(crate) fn mixed_sources(tabs: &[&TabState], pushdown_on: bool, conns: &[DbConnection]) -> bool {
    pushdown_on
        && tabs
            .iter()
            .any(|t| t.db_origin.is_some() && t.source_has_more())
        && server_sources_for(tabs, pushdown_on, conns).is_none()
}

type TaskSlot<T> = Arc<Mutex<Option<Result<T, String>>>>;

/// A server analysis a dialog owns. The status bar finds it through
/// `busy_db_job` (spinner + Cancel). Dropping it (dialog closed, rerun)
/// cancels a statement still running.
pub(crate) struct ServerTask<T> {
    pub(crate) load: super::state::DbLoadJob,
    slot: TaskSlot<T>,
}

pub(crate) enum TaskPoll<T> {
    Pending,
    Ready(T),
    Cancelled,
    Failed(String),
}

impl<T> ServerTask<T> {
    /// Takes the outcome; the caller drops the task after anything but
    /// `Pending`.
    pub(crate) fn poll(&self) -> TaskPoll<T> {
        // Read `finished` before the slot: the worker fills it first.
        let finished = self.load.finished.load(Ordering::Acquire);
        let taken = self.slot.lock().ok().and_then(|mut g| g.take());
        if self.load.cancelled.load(Ordering::Relaxed) && (taken.is_some() || finished) {
            return TaskPoll::Cancelled;
        }
        match taken {
            Some(Ok(v)) => TaskPoll::Ready(v),
            Some(Err(e)) => TaskPoll::Failed(e),
            None if finished => TaskPoll::Failed(t("pushdown.stopped")),
            None => TaskPoll::Pending,
        }
    }

    /// [`poll`](Self::poll) for a caller that treats every ending alike:
    /// `None` while the work runs, else its value or why there is none.
    pub(crate) fn poll_result(&self) -> Option<Result<T, String>> {
        match self.poll() {
            TaskPoll::Pending => None,
            TaskPoll::Ready(v) => Some(Ok(v)),
            TaskPoll::Failed(e) => Some(Err(e)),
            TaskPoll::Cancelled => Some(Err(t("pushdown.stopped"))),
        }
    }
}

impl<T> Drop for ServerTask<T> {
    fn drop(&mut self) {
        self.load.cancel_now();
    }
}

/// Run `work` on a worker with no connection: a dialog's pass over every
/// loaded row, which can take seconds. Polled and dropped like a server task;
/// dropping it only discards the result, since the work has no cancel. Not a
/// database job, so the status bar does not list it.
pub(crate) fn spawn_local_task<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> ServerTask<T> {
    spawn_local_fallible(move || Ok(work()))
}

/// [`spawn_local_task`] for work that can fail: the error comes back as
/// [`TaskPoll::Failed`], with its whole chain.
pub(crate) fn spawn_local_fallible<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> ServerTask<T> {
    let load = super::state::DbLoadJob::new(String::new());
    let finished = load.finished.clone();
    let slot: TaskSlot<T> = Arc::new(Mutex::new(None));
    let out = slot.clone();
    std::thread::spawn(move || {
        // Raised last, also on a panic: the poll reads that as "stopped".
        let _done = crate::app::flag_guard::FlagOnDrop::new(finished, true);
        let v = work().map_err(|e| format!("{e:#}"));
        if let Ok(mut g) = out.lock() {
            *g = Some(v);
        }
    });
    ServerTask { load, slot }
}

impl OctaApp {
    /// Run `work` on `conn` on a worker, with the vendor cancel wired up
    /// exactly as `start_pushdown` does it. No repaint from the worker: a
    /// dialog requests one each frame while its task is `Pending`.
    ///
    /// `work` may run twice: `with_conn` retries once on a cached connector
    /// that fails. It must therefore build up no state it captures, and it
    /// is not called at all once Cancel was pressed.
    pub(crate) fn spawn_server_task<T: Send + 'static>(
        &self,
        conn: DbConnection,
        hint: String,
        mut work: impl FnMut(&mut dyn octa::db::DbConnector, &AtomicBool) -> anyhow::Result<T>
        + Send
        + 'static,
    ) -> ServerTask<T> {
        let load = super::state::DbLoadJob::new(format!("{} {hint}", t("pushdown.running")));
        let finished = load.finished.clone();
        let cancel_slot = load.cancel.clone();
        let stop = load.cancelled.clone();
        let slot: TaskSlot<T> = Arc::new(Mutex::new(None));
        let out = slot.clone();
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        std::thread::spawn(move || {
            // First, so it is raised last, after the slot is filled, and also
            // on a panic: the poll reads "finished, slot empty" as a crash.
            let _done = crate::app::flag_guard::FlagOnDrop::new(finished, true);
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
            let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
            let res = cache
                .with_conn(&conn, secret.as_deref(), ssh.as_deref(), |c| {
                    // Before the handle goes in: a retry after Cancel must
                    // neither re-run `work` nor leave a handle behind.
                    octa::db::pushdown::check_cancel(&stop)?;
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = c.cancel_handle();
                    }
                    let r = work(c, &stop);
                    // Done with the connection: a stale handle must never be
                    // fired at the next job's statement on it.
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = None;
                    }
                    r
                })
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = out.lock() {
                *g = Some(res);
            }
        });
        ServerTask { load, slot }
    }

    /// The jobs the key-analysis dialogs and the database views own, for the
    /// status bar.
    pub(crate) fn dialog_tasks(&self) -> Vec<&super::state::DbLoadJob> {
        let keys = self.join_keys_dialog.as_ref();
        [
            keys.and_then(|d| d.counting.as_ref()).map(|s| &s.load),
            keys.and_then(|d| d.server.as_ref()).map(|s| &s.load),
            self.join_diag_dialog
                .as_ref()
                .and_then(|d| d.server.as_ref())
                .map(|s| &s.load),
            self.lookups_dialog
                .as_ref()
                .and_then(|d| d.server.as_ref())
                .map(|s| &s.load),
            self.lookups_dialog
                .as_ref()
                .and_then(|d| d.fetch.as_ref())
                .map(|s| &s.load),
            // Taken out during its own render, so the status bar sees it only
            // between frames; the dialog's spinner and Cancel cover the rest.
            self.rel_map_dialog
                .as_ref()
                .and_then(|d| d.measure.as_ref())
                .map(|s| &s.load),
            // Same: the Rolling question's row count.
            self.timeseries_dialog
                .as_ref()
                .and_then(|d| d.rolling_ask.as_ref())
                .and_then(|a| a.count.as_ref())
                .map(|s| &s.load),
        ]
        .into_iter()
        .flatten()
        .chain(
            self.tabs
                .iter()
                .filter_map(|t| t.view_task.as_ref().map(|(_, s)| &s.load)),
        )
        .chain(self.tabs.iter().filter_map(|t| {
            t.chart_server
                .as_ref()
                .and_then(|cs| cs.task.as_ref())
                .map(|s| &s.load)
        }))
        .chain(
            self.tabs
                .iter()
                .filter_map(|t| t.sql_import.as_ref().map(|i| &i.task.load)),
        )
        .collect()
    }
}

/// What a dialog shows under a result computed on the server.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DialogNote {
    pub(crate) unsaved: bool,
    /// Labels of parts computed on the loaded rows because the engine's SQL
    /// cannot express them (Join diagnostics on SQL Server).
    pub(crate) local: Vec<String>,
    /// Loaded rows those parts saw.
    pub(crate) loaded: usize,
    pub(crate) engine: Option<octa::db::DbEngine>,
    /// Join key finder, "Only the likely pairs".
    pub(crate) rescored: bool,
}

impl DialogNote {
    pub(crate) fn lines(&self) -> Vec<String> {
        let n = octa::ui::status_bar::format_number;
        let mut out = vec![t("pushdown.dialog_server")];
        if !self.local.is_empty() {
            out.push(
                t("pushdown.note_local")
                    .replace("{parts}", &self.local.join(", "))
                    .replace("{loaded}", &n(self.loaded))
                    .replace(
                        "{engine}",
                        self.engine.map(|e| e.label()).unwrap_or_default(),
                    ),
            );
        }
        if self.rescored {
            out.push(t("pushdown.keys_rescored_note"));
        }
        if self.unsaved {
            out.push(t("pushdown.note_unsaved"));
        }
        out
    }
}

/// The note, selectable so it can be copied.
pub(crate) fn dialog_note_ui(ui: &mut egui::Ui, note: &DialogNote) {
    let colour = ui.visuals().weak_text_color();
    for line in note.lines() {
        octa::ui::message::selectable_message(ui, colour, &line);
    }
}

pub(crate) enum ServerUi {
    Idle,
    Cancel,
    RunLocal,
}

/// Spinner and Cancel while the server works; after a refusal or a Cancel,
/// the message (selectable) and, when `offer_local`, Run on loaded rows.
pub(crate) fn server_status_ui(
    ui: &mut egui::Ui,
    running: bool,
    error: Option<&str>,
    offer_local: bool,
) -> ServerUi {
    let mut act = ServerUi::Idle;
    if running {
        octa::ui::control_row::control_row(ui, |ui| {
            ui.spinner();
            ui.label(t("pushdown.running"));
            if ui
                .button(t("common.cancel"))
                .on_hover_text(t("pushdown.cancel_hint"))
                .clicked()
            {
                act = ServerUi::Cancel;
            }
        });
    } else if let Some(e) = error {
        let colour = ui.visuals().error_fg_color;
        octa::ui::message::selectable_message(ui, colour, e);
        if offer_local
            && ui
                .button(t("pushdown.run_local"))
                .on_hover_text(t("pushdown.run_local_hint"))
                .clicked()
        {
            act = ServerUi::RunLocal;
        }
    }
    act
}

/// Column pairs across every pair of tables with these column counts.
pub(crate) fn column_pairs(widths: &[usize]) -> usize {
    widths
        .iter()
        .enumerate()
        .flat_map(|(i, a)| widths[i + 1..].iter().map(move |b| a * b))
        .sum()
}

/// The Join key finder's question before a whole-table run, in plain
/// words: each table's rows, the pairs, what is read, what it may cost.
pub(crate) fn cost_lines(
    tables: &[(String, octa::db::pushdown::row_estimate::RowCount)],
    pairs: usize,
) -> Vec<String> {
    let n = octa::ui::status_bar::format_number;
    let mut out: Vec<String> = tables
        .iter()
        .map(|(name, rc)| {
            let key = if rc.estimate {
                "pushdown.keys_cost_table_estimate"
            } else {
                "pushdown.keys_cost_table"
            };
            t(key)
                .replace("{table}", name)
                .replace("{rows}", &n(rc.rows))
        })
        .collect();
    out.push(t("pushdown.keys_cost_pairs").replace("{pairs}", &n(pairs)));
    out.push(t("pushdown.keys_cost_money"));
    out
}

/// Find lookup tables' question before Show breaking rows / Split out
/// after a server result.
pub(crate) fn fetch_prompt_lines(loaded: usize, total: usize) -> Vec<String> {
    let n = octa::ui::status_bar::format_number;
    vec![
        t("pushdown.fetch_body")
            .replace("{total}", &n(total))
            .replace("{loaded}", &n(loaded)),
        t("pushdown.fetch_why"),
    ]
}

/// Which live-database table a result came from, so its note's Load whole
/// table button finds the tab again after tabs moved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceKey {
    pub(crate) conn_id: String,
    pub(crate) catalog: Option<String>,
    pub(crate) schema: String,
    pub(crate) table: String,
}

impl SourceKey {
    pub(crate) fn of(o: &DbOrigin) -> Self {
        Self {
            conn_id: o.conn_id.clone(),
            catalog: o.catalog.clone(),
            schema: o.schema.clone(),
            table: o.table.clone(),
        }
    }

    pub(crate) fn find(&self, tabs: &[TabState]) -> Option<usize> {
        tabs.iter().position(|t| {
            t.db_origin
                .as_ref()
                .is_some_and(|o| SourceKey::of(o) == *self)
        })
    }
}

/// The banner a server-side result carries: what it covers and what it
/// does not. Not dismissable, like the partial note it replaces.
#[derive(Debug, Clone)]
pub(crate) struct PushdownNote {
    pub(crate) total: usize,
    pub(crate) loaded: usize,
    pub(crate) engine: octa::db::DbEngine,
    pub(crate) local: LocalParts,
    pub(crate) unsaved_edits: bool,
    pub(crate) source: SourceKey,
    /// The result was cut at the row cap: the database has more.
    pub(crate) more: bool,
}

pub(crate) struct NoteLines {
    pub(crate) server: String,
    pub(crate) local: Option<String>,
    pub(crate) unsaved: Option<String>,
    pub(crate) more: Option<String>,
}

impl PushdownNote {
    pub(crate) fn lines(&self) -> NoteLines {
        let n = octa::ui::status_bar::format_number;
        NoteLines {
            server: t("pushdown.note_server").replace("{total}", &n(self.total)),
            local: (!self.local.is_empty()).then(|| {
                let line = |key: &str, parts: &[String]| {
                    (!parts.is_empty()).then(|| {
                        t(key)
                            .replace("{parts}", &parts.join(", "))
                            .replace("{loaded}", &n(self.loaded))
                            .replace("{engine}", self.engine.label())
                    })
                };
                [
                    line("pushdown.note_local_by_design", &self.local.by_design),
                    line("pushdown.note_local", &self.local.engine),
                    line("pushdown.note_local_failed", &self.local.failed),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
            }),
            unsaved: self.unsaved_edits.then(|| t("pushdown.note_unsaved")),
            more: self.more.then(|| {
                let cap =
                    octa::db::pushdown::lookups::fetch_cap(octa::formats::initial_load_rows());
                t("pushdown.note_more").replace("{n}", &n(cap))
            }),
        }
    }
}

/// A Pivot, Resample or Rolling window run on the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReshapeSpec {
    Pivot(octa::db::pushdown::pivot::PivotSpec),
    Resample {
        /// Every column of the tab, for the bucket column's name.
        columns: Vec<String>,
        spec: octa::data::timeseries::ResampleSpec,
    },
    Rolling {
        spec: octa::data::timeseries::RollingSpec,
        /// The table key, ordering rows of equal time.
        tie: Vec<String>,
        /// Counted for the question before it ran: the note's total.
        total: usize,
    },
}

/// A reshape and what "Run on loaded rows" does instead: the file path's
/// DuckDB SQL, the tab label's verb, the status key if that fails.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReshapeJob {
    pub(crate) spec: ReshapeSpec,
    pub(crate) local_sql: String,
    pub(crate) label: String,
    pub(crate) failed_key: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PushdownKind {
    Summary,
    Quality,
    Correlation(CorrMethod),
    Reshape(Box<ReshapeJob>),
}

pub(crate) enum PushdownOutput {
    Summary(DataTable),
    Quality(QualityReport),
    Correlation(CorrMatrix),
    /// A reshape's table; the flag says it was cut at the row cap.
    Table(DataTable, bool),
}

type Done = Result<(PushdownOutput, usize, LocalParts), String>;

pub(crate) struct PushdownJob {
    kind: PushdownKind,
    note: PushdownNote,
    source_label: String,
    /// Spinner hint, exit flag, cancel closure and Cancel flag. Its own slot,
    /// not `db_load_job`, so table reads cannot overwrite it.
    pub(crate) load: super::state::DbLoadJob,
    slot: Arc<Mutex<Option<Done>>>,
}

pub(crate) struct PushdownError {
    message: String,
    kind: PushdownKind,
    source: SourceKey,
}

impl OctaApp {
    /// Run `kind` on the server for the active tab. One at a time; table
    /// reads may run beside it. The status bar shows it (`busy_db_job`).
    pub(crate) fn start_pushdown(
        &mut self,
        ctx: &egui::Context,
        kind: PushdownKind,
        src: ServerSource,
    ) {
        if self.pushdown_job.is_some() {
            self.status_message = Some((t("pushdown.busy_hint"), std::time::Instant::now()));
            return;
        }
        let tab = &self.tabs[self.active_tab];
        let Some(origin) = tab.db_origin.as_ref() else {
            return;
        };
        let mut loaded = tab.table.clone();
        loaded.apply_edits();
        let note = PushdownNote {
            total: 0,
            loaded: tab.table.rows.len(),
            engine: src.engine(),
            local: LocalParts::default(),
            unsaved_edits: tab.table.is_modified(),
            source: SourceKey::of(origin),
            more: false,
        };
        let source_label = tab.result_source_label();
        let enabled = self.settings.summary_stats.clone();
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let load =
            super::state::DbLoadJob::new(format!("{} {source_label}", t("pushdown.running")));
        let finished = load.finished.clone();
        let cancel_slot = load.cancel.clone();
        let stop = load.cancelled.clone();
        let slot: Arc<Mutex<Option<Done>>> = Arc::new(Mutex::new(None));
        let out = slot.clone();
        let ctx = ctx.clone();
        let job_kind = kind.clone();
        let cap = octa::db::pushdown::lookups::fetch_cap(octa::formats::initial_load_rows());
        std::thread::spawn(move || {
            // First, so it is raised last, after the slot is filled, and also
            // on a panic: the drain reads "finished, slot empty" as a crash.
            let _done = crate::app::flag_guard::FlagOnDrop::new(finished, true);
            let secret = octa::ui::settings::db_secrets::get_db_secret(&src.conn.id, &settings);
            let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&src.conn.id, &settings);
            let res = cache
                .with_conn(&src.conn, secret.as_deref(), ssh.as_deref(), |c| {
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = c.cancel_handle();
                    }
                    use octa::db::pushdown as p;
                    // Checks `stop` first, which also ends `with_conn`'s one
                    // retry after a Cancel.
                    let total = match &kind {
                        PushdownKind::Reshape(job) => match &job.spec {
                            ReshapeSpec::Rolling { total, .. } => Ok(*total),
                            _ => p::count_rows(c, &src, &stop),
                        },
                        _ => p::count_rows(c, &src, &stop),
                    };
                    let res = total.and_then(|total| {
                        Ok(match &kind {
                            PushdownKind::Summary => {
                                let (table, local) =
                                    p::summary::run(c, &src, &loaded, total, &enabled, &stop)?;
                                (PushdownOutput::Summary(table), total, local)
                            }
                            PushdownKind::Quality => {
                                let (report, local) =
                                    p::quality::run(c, &src, &loaded, total, &stop)?;
                                (PushdownOutput::Quality(report), total, local)
                            }
                            PushdownKind::Correlation(method) => {
                                // The loaded rows pick the numeric columns, as the
                                // in-memory engine does: Postgres NUMERIC and MySQL
                                // DECIMAL arrive as Utf8.
                                let cols: Vec<_> =
                                    octa::data::correlation::numeric_columns(&loaded)
                                        .into_iter()
                                        .map(|i| loaded.columns[i].clone())
                                        .collect();
                                let m = p::correlation::run(c, &src, &cols, *method, &stop)?;
                                (PushdownOutput::Correlation(m), total, LocalParts::default())
                            }
                            PushdownKind::Reshape(job) => {
                                let (table, more) = match &job.spec {
                                    ReshapeSpec::Pivot(spec) => p::pivot::run(
                                        c,
                                        &src,
                                        spec,
                                        p::pivot::chunk_size(spec.group.len()),
                                        cap,
                                        &stop,
                                    )?,
                                    ReshapeSpec::Resample { columns, spec } => {
                                        p::timeseries::resample(c, &src, columns, spec, cap, &stop)?
                                    }
                                    ReshapeSpec::Rolling { spec, tie, .. } => {
                                        p::timeseries::rolling(c, &src, spec, tie, cap, &stop)?
                                    }
                                };
                                (
                                    PushdownOutput::Table(table, more),
                                    total,
                                    LocalParts::default(),
                                )
                            }
                        })
                    });
                    // Done with the connection: a stale handle must never be
                    // fired at the next job's statement on it.
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = None;
                    }
                    res
                })
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = out.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        });
        self.pushdown_job = Some(PushdownJob {
            kind: job_kind,
            note,
            source_label,
            load,
            slot,
        });
    }

    /// Open the finished result. Called once per frame from the update loop.
    pub(crate) fn drain_pushdown_job(&mut self) {
        let Some(j) = &self.pushdown_job else {
            return;
        };
        // Read `finished` before the slot: the worker fills the slot first.
        let finished = j.load.finished.load(Ordering::Acquire);
        let done = match j.slot.lock().ok().and_then(|mut g| g.take()) {
            Some(done) => done,
            None if finished => Err(t("pushdown.stopped")),
            None => return,
        };
        let Some(job) = self.pushdown_job.take() else {
            return;
        };
        match done {
            Ok((output, total, local)) => {
                let note = PushdownNote {
                    total,
                    local,
                    ..job.note
                };
                match (output, job.kind) {
                    (PushdownOutput::Summary(table), _) => {
                        self.push_summary_tab(table, &job.source_label, None, Some(note))
                    }
                    (PushdownOutput::Quality(report), _) => {
                        self.push_quality_tabs(report, &job.source_label, None, Some(note))
                    }
                    (PushdownOutput::Correlation(m), PushdownKind::Correlation(method)) => {
                        self.push_correlation_tab(m, method, &job.source_label, None, Some(note))
                    }
                    (PushdownOutput::Table(table, more), PushdownKind::Reshape(r)) => {
                        let note = PushdownNote { more, ..note };
                        self.push_reshape_tab(table, &r.label, &job.source_label, note)
                    }
                    (PushdownOutput::Table(..), _) => {}
                    (PushdownOutput::Correlation(_), _) => {}
                }
            }
            Err(_) if job.load.cancelled.load(Ordering::Relaxed) => {
                self.status_message = Some((t("db.load_cancelled"), std::time::Instant::now()));
            }
            Err(message) => {
                self.pushdown_error = Some(PushdownError {
                    message,
                    kind: job.kind,
                    source: job.note.source,
                });
            }
        }
    }

    /// The server refused: say why, offer the loaded rows. Never falls back
    /// on its own, so a permissions error cannot pass for an answer.
    pub(crate) fn render_pushdown_error(&mut self, ctx: &egui::Context) {
        let Some(err) = &self.pushdown_error else {
            return;
        };
        let source_open = err.source.find(&self.tabs).is_some();
        let dialog_id = egui::Id::new("octa_pushdown_error");
        let size_key = dialog_id.with("octa_dlg_size");
        let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
        let minimized = size == DialogSize::Minimized;
        let mut close = false;
        let mut run_local = false;
        let center = center_on_first_show(ctx, egui::vec2(480.0, 240.0));
        let window = egui::Window::new("octa_pushdown_error")
            .id(dialog_id)
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(480.0)
                .default_height(240.0)
                .min_width(320.0)
                .min_height(160.0)
                .default_pos(center)
        });
        let inner = window.show(ctx, |ui| {
            egui::Panel::top("octa_pushdown_error_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.label(
                            egui::RichText::new(t("pushdown.error_title"))
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
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(t("pushdown.error_body"));
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        let colour = ui.visuals().error_fg_color;
                        octa::ui::message::selectable_message(ui, colour, &err.message);
                    });
                ui.add_space(12.0);
                octa::ui::control_row::control_row(ui, |ui| {
                    let b = ui.add_enabled(source_open, egui::Button::new(t("pushdown.run_local")));
                    run_local = b.clicked();
                    if source_open {
                        b.on_hover_text(t("pushdown.run_local_hint"));
                    } else {
                        b.on_disabled_hover_text(t("pushdown.source_closed_hint"));
                    }
                    if ui
                        .button(t("common.close"))
                        .on_hover_text(t("pushdown.close_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        });
        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                size_key,
                if close || run_local {
                    DialogSize::Normal
                } else {
                    size
                },
            )
        });
        if run_local {
            let Some(err) = self.pushdown_error.take() else {
                return;
            };
            if let Some(idx) = err.source.find(&self.tabs) {
                self.active_tab = idx;
                match err.kind {
                    PushdownKind::Summary => self.open_describe_tab_local(),
                    PushdownKind::Quality => self.open_quality_tab_local(),
                    PushdownKind::Correlation(m) => self.open_correlation_tab_local(m),
                    PushdownKind::Reshape(r) => {
                        self.open_reshape_tab_local(&r.local_sql, &r.label, r.failed_key, None)
                    }
                }
            }
        } else if close {
            self.pushdown_error = None;
        }
    }
}

/// Column index AND name: a structural edit that shifts columns while the
/// dialog is open must not show the old column's counts under a new name.
type VfKey = (usize, String, Option<usize>, BinningMode);
/// Shared, so the per-frame poll hands out a pointer, not a copy of up to
/// millions of labels.
type VfDone = Result<Arc<ValueFrequency>, String>;

/// The value-frequency dialog's server count for one tab: what was asked,
/// the job running it, and the answer once it arrives. A changed column,
/// Top-N or binning is a new key, so a new query. Dropping it (dialog
/// closed, key changed, tab closed) cancels a count still running, so no
/// worker holds the shared connector unseen.
pub(crate) struct VfServer {
    key: VfKey,
    /// Its own job, not `db_load_job`, so a table read cannot take its
    /// spinner or Cancel. `busy_db_job` finds it while it runs.
    pub(crate) load: super::state::DbLoadJob,
    slot: Arc<Mutex<Option<VfDone>>>,
    result: Option<VfDone>,
    /// "Run on loaded rows" was clicked after an error or a Cancel.
    local: bool,
}

impl VfServer {
    fn poll(&mut self) -> ServerVf {
        if self.local {
            return ServerVf::Local;
        }
        if self.result.is_none() {
            // Read `finished` before the slot: the worker fills it first.
            let finished = self.load.finished.load(Ordering::Acquire);
            self.result = match self.slot.lock().ok().and_then(|mut g| g.take()) {
                Some(done) => Some(done),
                None if finished => Some(Err(t("pushdown.stopped"))),
                None => None,
            };
        }
        match &self.result {
            None => ServerVf::Pending,
            // The engine's first query is COUNT(*), so nulls + non-null is
            // the table's row count: no separate count_rows round trip.
            Some(Ok(v)) => ServerVf::Ready(Arc::clone(v), v.nulls + v.total_non_null),
            Some(Err(_)) if self.load.cancelled.load(Ordering::Relaxed) => ServerVf::Cancelled,
            Some(Err(e)) => ServerVf::Failed(e.clone()),
        }
    }
}

impl Drop for VfServer {
    fn drop(&mut self) {
        self.load.cancel_now();
    }
}

pub(crate) enum ServerVf {
    /// Not a server tab, or the user chose the loaded rows: compute in
    /// memory as before.
    Local,
    Pending,
    /// The counts and the table's row count.
    Ready(Arc<ValueFrequency>, usize),
    Cancelled,
    Failed(String),
}

impl OctaApp {
    /// The value-frequency counts for `col_idx`, from the server when the
    /// tab is a partial live-database tab. Starts the count on first ask and
    /// whenever the key changes (cancelling the old one); polls it after.
    pub(crate) fn server_value_frequency(
        &mut self,
        ctx: &egui::Context,
        tab_idx: usize,
        col_idx: usize,
        top_n: Option<usize>,
        binning: BinningMode,
    ) -> ServerVf {
        let Some(src) = server_source_for(
            &self.tabs[tab_idx],
            self.settings.db_pushdown,
            &self.settings.db_connections,
        ) else {
            return ServerVf::Local;
        };
        let tab = &mut self.tabs[tab_idx];
        let col = tab.table.columns[col_idx].clone();
        let key = (col_idx, col.name.clone(), top_n, binning);
        if let Some(vf) = tab.vf_server.as_mut().filter(|v| v.key == key) {
            return vf.poll();
        }
        let load = super::state::DbLoadJob::new(format!("{} {}", t("pushdown.running"), col.name));
        let finished = load.finished.clone();
        let cancel_slot = load.cancel.clone();
        let stop = load.cancelled.clone();
        let slot: Arc<Mutex<Option<VfDone>>> = Arc::new(Mutex::new(None));
        let out = slot.clone();
        // Replacing the old state drops it, which cancels its count.
        tab.vf_server = Some(VfServer {
            key,
            load,
            slot,
            result: None,
            local: false,
        });
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // First, so it is raised last, after the slot is filled, and also
            // on a panic: the poll reads "finished, slot empty" as a crash.
            let _done = crate::app::flag_guard::FlagOnDrop::new(finished, true);
            let secret = octa::ui::settings::db_secrets::get_db_secret(&src.conn.id, &settings);
            let ssh = octa::ui::settings::db_secrets::get_ssh_secret(&src.conn.id, &settings);
            let res = cache
                .with_conn(&src.conn, secret.as_deref(), ssh.as_deref(), |c| {
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = c.cancel_handle();
                    }
                    let v = octa::db::pushdown::value_frequency::run(
                        c, &src, &col, top_n, binning, &stop,
                    );
                    // Done with the connection: a stale handle must never be
                    // fired at the next job's statement on it.
                    if let Ok(mut s) = cancel_slot.lock() {
                        *s = None;
                    }
                    v.map(Arc::new)
                })
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = out.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        });
        ServerVf::Pending
    }

    /// Count the dialog's column on the loaded rows instead, until the key
    /// changes or the dialog closes.
    pub(crate) fn value_frequency_run_local(&mut self, tab_idx: usize) {
        if let Some(vf) = self.tabs[tab_idx].vf_server.as_mut() {
            vf.local = true;
        }
    }

    /// The dialog closed: forget the answer (dropping it stops a count
    /// still running), so reopening counts afresh.
    pub(crate) fn value_frequency_closed(&mut self, tab_idx: usize) {
        self.tabs[tab_idx].vf_server = None;
    }
}

#[cfg(test)]
#[path = "pushdown_tests.rs"]
mod tests;
