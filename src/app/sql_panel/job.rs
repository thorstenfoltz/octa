//! Local SQL off the window thread: copying the tab into `data` and running a
//! query both happen on a worker holding the workspace's second connection
//! (`SqlWorkspace::run_job`). A query over a few million rows, or the copy of
//! them (~1.3 s per 1M rows), used to freeze the window until it returned.
//!
//! The copy is fed a slice per frame from the window thread, as a background
//! save is (`SliceFeed`), so no whole copy of the table is ever made. While a
//! copy runs, or a statement that writes, the tab holds its rows
//! (`TabState::holds_rows`): read-only, no sorting, no rows streamed in, so
//! the copy is of one table and a write folds back over the table it read.
//! A plain SELECT leaves the tab alone.

use std::sync::Arc;

use super::*;
use crate::app::pushdown::{ServerTask, TaskPoll, spawn_local_task};
use crate::app::slice_feed::{RowsStamp, SliceFeed, slice_rows};
use octa::sql::{DataFeed, JobQuery, RunDone};

/// A copy of `data` and/or a query running for one tab.
pub(crate) struct SqlJob {
    /// The editor that asked for the query; `None` for a copy alone.
    pane_id: Option<u64>,
    query: Option<String>,
    started: std::time::Instant,
    /// The tab was read-only when the query started, so a write it made is
    /// not folded back.
    readonly: bool,
    /// The copy being fed, the table it is of, and the stamp `data` gets
    /// once it landed. `None` when `data` was current already.
    feed: Option<(SliceFeed, DataStamp)>,
    task: ServerTask<RunDone>,
    interrupt: Arc<duckdb::InterruptHandle>,
    /// Explain this query in that editor once the copy landed: Explain
    /// found `data` out of date.
    pub(super) then_explain: Option<(u64, String)>,
}

impl SqlJob {
    /// Whether the tab must keep its rows as they are until this lands.
    pub(crate) fn holds_rows(&self) -> bool {
        self.feed.is_some() || self.query.as_deref().is_some_and(octa::sql::is_mutation)
    }

    /// Whether this is a query (not just a copy), for the Cancel button.
    pub(crate) fn runs_query(&self) -> bool {
        self.query.is_some()
    }

    /// Stop it: no more rows for the copy, and the query interrupted.
    pub(crate) fn cancel(&mut self) {
        if let Some((feed, ..)) = self.feed.as_mut() {
            feed.tx = None;
        }
        self.interrupt.interrupt();
    }
}

impl Drop for SqlJob {
    /// The tab closed or Octa is exiting. An interrupt reaching an idle
    /// connection could stop its next query, so only a running one is.
    fn drop(&mut self) {
        if !self
            .task
            .load
            .finished
            .load(std::sync::atomic::Ordering::Acquire)
        {
            self.interrupt.interrupt();
        }
    }
}

impl OctaApp {
    /// Copy tab `tab_idx` into `data` when it is not there as it is now,
    /// then run `query` when given, in the background. The query's result
    /// lands in the editor that is active now.
    pub(crate) fn start_sql_job(&mut self, tab_idx: usize, query: Option<String>) {
        let page_rows = self.settings.sql_result_page_rows;
        let readonly = self.is_readonly();
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        if tab.sql_job.is_some() {
            if query.is_some() {
                tab.sql.running_since = None;
                tab.sql.error = Some(octa::i18n::t("sql.busy"));
            }
            return;
        }
        ensure_workspace(tab);
        // A large tab's `data` is a view over its file: nothing to copy.
        let copy = if tab.large.is_some() {
            if let Err(e) = register_active_data(tab) {
                tab.sql.running_since = None;
                tab.sql.error = Some(format!("{e:#}"));
                return;
            }
            false
        } else {
            data_is_stale(tab)
        };
        if !copy && query.is_none() {
            return;
        }
        let Some(ws) = tab.sql_workspace.as_ref() else {
            // `ensure_workspace` put the reason in the pane.
            tab.sql.running_since = None;
            return;
        };
        let (feed, data_feed) = if copy {
            let rows = slice_rows(tab.table.col_count());
            let (feed, rx) = SliceFeed::new(&tab.table, None, None, rows);
            let data_feed = DataFeed {
                columns: tab.table.columns.clone(),
                rows: rx,
            };
            (Some((feed, DataStamp::of(&tab.table))), Some(data_feed))
        } else {
            (None, None)
        };
        let job_query = query.as_ref().map(|sql| JobQuery {
            sql: sql.clone(),
            page_rows,
            slot: tab.sql.id,
        });
        match ws.run_job(data_feed, job_query) {
            Ok((work, interrupt)) => {
                if copy {
                    tab.sql_data_stamp = None;
                }
                tab.sql_job = Some(SqlJob {
                    pane_id: query.is_some().then_some(tab.sql.id),
                    query,
                    started: std::time::Instant::now(),
                    readonly,
                    feed,
                    task: spawn_local_task(work),
                    interrupt,
                    then_explain: None,
                });
            }
            Err(e) => {
                tab.sql.running_since = None;
                tab.sql.error = Some(format!("{e:#}"));
            }
        }
    }

    /// Stop the active tab's local query or copy.
    pub(super) fn cancel_local_sql(&mut self) {
        if let Some(job) = self.tabs[self.active_tab].sql_job.as_mut() {
            job.cancel();
        }
    }

    /// Once per frame: feed every copy and land every finished job.
    pub(crate) fn drive_sql_jobs(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let tab = &mut self.tabs[idx];
            let Some(job) = tab.sql_job.as_mut() else {
                continue;
            };
            if let Some((feed, stamp)) = job.feed.as_mut() {
                if *stamp.rows() == RowsStamp::of(&tab.table) {
                    feed.feed(&tab.table);
                } else {
                    // Another table now: stop, `data` stays as it was.
                    feed.tx = None;
                }
            }
            let done = match job.task.poll() {
                TaskPoll::Pending => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                    continue;
                }
                TaskPoll::Ready(done) => done,
                // The worker panicked.
                TaskPoll::Failed(e) => RunDone {
                    copied: job.feed.as_ref().map(|_| Err(e.clone())),
                    query: job.query.as_ref().map(|_| Err(e)),
                },
                TaskPoll::Cancelled => RunDone {
                    copied: None,
                    query: job
                        .query
                        .as_ref()
                        .map(|_| Err(octa::i18n::t("pushdown.stopped"))),
                },
            };
            let job = tab.sql_job.take().expect("borrowed above");
            self.land_sql_job(ctx, idx, job, done);
        }
    }

    fn land_sql_job(&mut self, ctx: &egui::Context, idx: usize, mut job: SqlJob, done: RunDone) {
        let tab = &mut self.tabs[idx];
        let feed = job.feed.take();
        if let Some((pane_id, query)) = job.then_explain.take() {
            let copied = match &done.copied {
                Some(Err(e)) => Err(anyhow::anyhow!("{e}")),
                _ => Ok(()),
            };
            if copied.is_ok()
                && let (Some(ws), Some((_, stamp))) = (tab.sql_workspace.as_mut(), feed.as_ref())
                && let Some(Ok(rows)) = &done.copied
            {
                ws.note_active_table(*rows);
                tab.sql_data_stamp = Some(stamp.clone());
            }
            let planned = copied.and_then(|()| match tab.sql_workspace.as_mut() {
                Some(ws) => ws.explain(&query),
                None => Err(anyhow::anyhow!("the SQL workspace is gone")),
            });
            if let Some(pane) = tab.sql_pane_by_id_mut(pane_id) {
                super::explain::show_planned(pane, planned);
            }
            return;
        }
        match done.copied {
            Some(Ok(rows)) => {
                if let Some(ws) = tab.sql_workspace.as_mut() {
                    ws.note_active_table(rows);
                }
                tab.sql_data_stamp = feed.map(|(_, stamp)| stamp);
            }
            Some(Err(e)) => {
                // A copy stopped because the table changed is no error: the
                // next Run copies the table as it is then.
                let changed =
                    feed.is_some_and(|(_, stamp)| *stamp.rows() != RowsStamp::of(&tab.table));
                let message = if changed {
                    // Only a Run is told: the copy on panel open just waits
                    // for the next one.
                    job.pane_id.map(|_| octa::i18n::t("sql.copy_changed"))
                } else {
                    Some(e)
                };
                let id = job.pane_id.unwrap_or(tab.sql.id);
                if let (Some(message), Some(pane)) = (message, tab.sql_pane_by_id_mut(id)) {
                    pane.running_since = None;
                    pane.error = Some(message);
                }
            }
            None => {}
        }
        let (Some(pane_id), Some(query), Some(outcome)) =
            (job.pane_id, job.query.take(), done.query)
        else {
            return;
        };
        // The result goes to the editor that ran it, wherever it is now.
        let tab = &mut self.tabs[idx];
        let active = tab.sql_active_pane;
        let slot = if tab.sql.id == pane_id {
            Some(active)
        } else {
            (0..tab.sql_panes.len()).find(|&i| i != active && tab.sql_panes[i].id == pane_id)
        };
        let Some(slot) = slot else {
            return;
        };
        tab.activate_sql_pane(slot);
        tab.sql.running_since = None;
        tab.sql.last_duration_ms = Some(job.started.elapsed().as_millis() as u64);
        self.land_sql_query(ctx, idx, query, job.started, job.readonly, outcome);
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.activate_sql_pane(active);
        }
    }
}

/// Whether `data` must be copied again before a query reads it: never
/// copied, removed in the workspace list, or the table changed since.
pub(super) fn data_is_stale(tab: &TabState) -> bool {
    if tab.table.col_count() == 0 {
        return false;
    }
    let registered = tab
        .sql_workspace
        .as_ref()
        .is_some_and(|ws| ws.list_tables().iter().any(|t| t.sql_name == "data"));
    !registered || tab.sql_data_stamp.as_ref() != Some(&DataStamp::of(&tab.table))
}
