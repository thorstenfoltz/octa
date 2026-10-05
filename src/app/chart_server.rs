//! A chart opened from a database tab that does not hold every row: it
//! remembers where its rows came from and asks the database for each chart
//! (`octa::db::pushdown::chart`). A change to the chart's data settings goes
//! out after the typing pause; the previous answer stays on screen until
//! the new one arrives.

use std::time::{Duration, Instant};

use eframe::egui;
use octa::data::chart::{ChartError, ChartLimits, ChartPrep};
use octa::db::DbConnection;
use octa::db::pushdown::chart::{ChartKey, ChartRequest, ColumnRead, ServerChart, column_reads};
use octa::db::pushdown::{LocalParts, ServerSource};
use octa::i18n::t;

use super::db_view::SETTLE;
use super::pushdown::{PushdownNote, ServerTask, SourceKey, TaskPoll, server_source_for};
use super::state::{OctaApp, TabState};

/// The chart tab's link to its database table.
pub(crate) struct ChartServer {
    /// The table and the source tab's server filter, frozen at opening.
    pub(crate) src: ServerSource,
    pub(crate) source: SourceKey,
    /// How the local chart reads each column (from the loaded rows).
    pub(crate) reads: Vec<ColumnRead>,
    /// The table key, ordering rows of equal X for Line / Scatter.
    pub(crate) tie: Vec<String>,
    /// Rows the chart tab copied, for the notes.
    pub(crate) loaded: usize,
    /// The source had unsaved edits the database has not seen.
    pub(crate) unsaved: bool,
    /// A changed key and when it was first wanted (the typing pause).
    pub(crate) settle: Option<(ChartKey, Instant)>,
    /// The key the running task answers.
    pub(crate) pending: Option<ChartKey>,
    /// Dropping it cancels the statement still running.
    pub(crate) task: Option<ServerTask<ServerChart>>,
    pub(crate) shown: Option<(ChartKey, ServerChart)>,
    /// The database refused (or the user cancelled) this key, with the
    /// message to show; waits for Try again.
    pub(crate) error: Option<(ChartKey, String)>,
    /// "Use the loaded rows": draw locally until Try again.
    pub(crate) local: bool,
}

/// The chart server for a chart opened from `tab`, or `None` where the
/// chart draws from its copied rows as always: a file, cloud, API or SQL
/// result tab, a database tab holding every row, the setting off.
pub(crate) fn chart_server_for(
    tab: &TabState,
    pushdown_on: bool,
    conns: &[DbConnection],
) -> Option<ChartServer> {
    let src = server_source_for(tab, pushdown_on, conns)?;
    let origin = tab.db_origin.as_ref()?;
    let tie = match &origin.identity {
        Some(octa::db::write_back::RowIdentity::Key(k)) => k.clone(),
        _ => Vec::new(),
    };
    Some(ChartServer {
        src,
        source: SourceKey::of(origin),
        reads: column_reads(&tab.table),
        tie,
        loaded: tab.table.rows.len(),
        unsaved: tab.table.is_modified(),
        settle: None,
        pending: None,
        task: None,
        shown: None,
        error: None,
        local: false,
    })
}

pub(crate) enum ChartStep {
    /// Answered, asked, refused, local, or not enough picked yet.
    Idle,
    /// The settings may still be changing: look again after this long.
    Settle(Duration),
    Query(ChartKey),
}

/// What to do about `wanted` now. The first chart goes out at once; a
/// change waits until it has stood for [`SETTLE`].
pub(crate) fn chart_step(cs: &mut ChartServer, wanted: &ChartKey, now: Instant) -> ChartStep {
    if cs.local || !wanted.complete() || cs.pending.as_ref() == Some(wanted) {
        return ChartStep::Idle;
    }
    let answered = cs.shown.as_ref().is_some_and(|(k, _)| k == wanted);
    let refused = cs.error.as_ref().is_some_and(|(k, _)| k == wanted);
    if answered || refused {
        // Back to the shown key: a query still running for another one
        // would replace it. Dropping the task cancels its statement.
        cs.settle = None;
        cs.pending = None;
        cs.task = None;
        return ChartStep::Idle;
    }
    if cs.shown.is_none() && cs.pending.is_none() && cs.error.is_none() {
        return ChartStep::Query(wanted.clone());
    }
    match &cs.settle {
        Some((k, since)) if k == wanted => {
            let left = SETTLE.saturating_sub(now.saturating_duration_since(*since));
            if left.is_zero() {
                cs.settle = None;
                ChartStep::Query(wanted.clone())
            } else {
                ChartStep::Settle(left)
            }
        }
        _ => {
            cs.settle = Some((wanted.clone(), now));
            ChartStep::Settle(SETTLE)
        }
    }
}

/// What the chart view draws.
pub(crate) enum ChartShow {
    /// The copied rows: by choice, or not enough picked for a query yet.
    Local,
    /// Nothing from the database yet.
    Waiting,
    /// The database cannot draw this one: the copied rows, with the reason.
    NotExpressible,
    /// The database's chart (the previous one while a new one is fetched).
    Drawn(Result<ChartPrep, ChartError>),
}

impl ChartServer {
    fn refused(&self, wanted: &ChartKey) -> bool {
        self.error.as_ref().is_some_and(|(k, _)| k == wanted)
    }

    pub(crate) fn show(&self, wanted: &ChartKey) -> ChartShow {
        if self.local || !wanted.complete() {
            return ChartShow::Local;
        }
        if self.refused(wanted) {
            // Not the previous chart under an error about these settings.
            return ChartShow::Waiting;
        }
        match &self.shown {
            Some((k, ServerChart::NotExpressible)) if k == wanted => ChartShow::NotExpressible,
            Some((_, ServerChart::Drawn { chart, .. })) => ChartShow::Drawn(chart.clone()),
            _ => ChartShow::Waiting,
        }
    }

    /// The "Computed on the database over all N rows" note for the tab.
    pub(crate) fn note(&self, wanted: &ChartKey) -> Option<PushdownNote> {
        if self.local || !wanted.complete() || self.refused(wanted) {
            return None;
        }
        match &self.shown {
            Some((_, ServerChart::Drawn { total, .. })) => Some(PushdownNote {
                total: *total,
                loaded: self.loaded,
                engine: self.src.engine(),
                local: LocalParts::default(),
                unsaved_edits: self.unsaved,
                source: self.source.clone(),
                more: false,
            }),
            _ => None,
        }
    }

    /// Take a finished answer; true while the task still runs.
    pub(crate) fn poll(&mut self) -> bool {
        let Some(task) = &self.task else {
            return false;
        };
        let answer = match task.poll() {
            TaskPoll::Pending => return true,
            TaskPoll::Ready(a) => Ok(a),
            TaskPoll::Cancelled => Err(t("pushdown.cancelled")),
            TaskPoll::Failed(e) => Err(format!("{} {e}", t("pushdown.chart_failed"))),
        };
        self.task = None;
        if let Some(key) = self.pending.take() {
            match answer {
                Ok(a) => {
                    self.shown = Some((key, a));
                    self.error = None;
                }
                Err(e) => self.error = Some((key, e)),
            }
        }
        false
    }
}

impl OctaApp {
    /// Once per frame: take finished chart answers (every tab), and send
    /// the active chart tab's settings once they have settled.
    pub(crate) fn sync_chart_server(&mut self, ctx: &egui::Context) {
        if !self.settings.db_pushdown {
            // Turned off: open charts draw from their copied rows, as if
            // opened with the setting off. Dropping a task cancels it.
            for tab in &mut self.tabs {
                if tab.chart_server.take().is_some() {
                    tab.pushdown_note = None;
                }
            }
            return;
        }
        let limits = ChartLimits {
            max_points: self.settings.chart_max_points,
            max_categories: self.settings.chart_max_categories,
        };
        for tab in &mut self.tabs {
            let wanted = ChartKey::of(&tab.chart_config, limits);
            let Some(cs) = tab.chart_server.as_mut() else {
                continue;
            };
            if cs.poll() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            tab.pushdown_note = cs.note(&wanted);
        }
        let idx = self.active_tab;
        let (key, req, src) = {
            let Some(tab) = self.tabs.get_mut(idx) else {
                return;
            };
            let wanted = ChartKey::of(&tab.chart_config, limits);
            let Some(cs) = tab.chart_server.as_mut() else {
                return;
            };
            match chart_step(cs, &wanted, Instant::now()) {
                ChartStep::Idle => return,
                ChartStep::Settle(wait) => {
                    ctx.request_repaint_after(wait);
                    return;
                }
                ChartStep::Query(key) => {
                    let req = ChartRequest {
                        key: key.clone(),
                        columns: tab.table.columns.clone(),
                        reads: cs.reads.clone(),
                        tie: cs.tie.clone(),
                    };
                    (key, req, cs.src.clone())
                }
            }
        };
        let hint = self.tabs[idx].chart_tab_label.clone().unwrap_or_default();
        let conn = src.conn.clone();
        let task = self.spawn_server_task(conn, hint, move |c, stop| {
            octa::db::pushdown::chart::run(c, &src, &req, stop)
        });
        if let Some(cs) = self.tabs[idx].chart_server.as_mut() {
            cs.pending = Some(key);
            cs.task = Some(task);
            cs.error = None;
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
#[path = "chart_server_tests.rs"]
mod tests;
