//! The finish notification's trigger. Each frame asks `long_task_label()`
//! whether anything long is running, and the busy -> idle edge of that answer
//! is the moment a notification can fire. The older jobs are read from their
//! own state; every other worker thread holds a [`LongTasks`] token, which
//! `tests/long_task_tokens_tests.rs` enforces.
//!
//! ponytail: overlapping tasks are one busy span and get one notification,
//! named after the first. Track spans per task if one-per-task is wanted.

use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use eframe::egui;

use super::state::OctaApp;

/// Jobs running on worker threads, each alive while its token is.
#[derive(Default)]
pub(crate) struct LongTasks(RefCell<Vec<(String, Weak<()>)>>);

impl LongTasks {
    /// A token for a job about to start: move it into the worker thread with
    /// `let _task = task;` (a bare `let _ = task;` does not move it), and the
    /// job counts as running until the thread ends. `label` names it in the
    /// notification.
    pub(crate) fn start(&self, label: String) -> Arc<()> {
        let token = Arc::new(());
        let mut tasks = self.0.borrow_mut();
        tasks.retain(|(_, alive)| alive.strong_count() > 0);
        tasks.push((label, Arc::downgrade(&token)));
        token
    }

    /// The oldest job still running.
    pub(crate) fn running(&self) -> Option<String> {
        let mut tasks = self.0.borrow_mut();
        tasks.retain(|(_, alive)| alive.strong_count() > 0);
        tasks.first().map(|(label, _)| label.clone())
    }
}

impl OctaApp {
    /// What long task is running now, in a few words, or `None`. The status
    /// bar's busy hint first, then the jobs the status bar does not show.
    pub(crate) fn long_task_label(&self) -> Option<String> {
        use octa::i18n::t;
        if let Some(hint) = self.status_busy_hint() {
            return Some(hint);
        }
        let sql_running = self.sql_server_job.is_some()
            || self.tabs.iter().any(|tab| {
                (0..tab.sql_pane_count()).any(|i| {
                    if i == tab.sql_active_pane {
                        tab.sql.running_since.is_some()
                    } else {
                        tab.sql_panes[i].running_since.is_some()
                    }
                })
            });
        let flag = |f: &std::sync::Arc<std::sync::atomic::AtomicBool>| f.load(Ordering::Relaxed);
        let label = if sql_running {
            "notify.task_sql"
        } else if self.full_scan_job.is_some() {
            "notify.task_full_scan"
        } else if self
            .batch_convert_dialog
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            "notify.task_batch_convert"
        } else if self
            .folder_compare
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            // The dialog's own title names the task well enough.
            "folder_compare.title"
        } else if self
            .schema_drift_dialog
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            "notify.task_schema_drift"
        } else if self
            .harmonise_dialog
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            "notify.task_harmonise"
        } else if self
            .report_dialog
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            "notify.task_report"
        } else if self
            .fuzzy_join_dialog
            .as_ref()
            .is_some_and(|s| flag(&s.running))
        {
            "notify.task_fuzzy_join"
        } else if self
            .cloud_transfer_dialog
            .as_ref()
            .is_some_and(|s| s.job.is_some())
        {
            "notify.task_cloud_transfer"
        } else if self
            .db_copy_dialog
            .as_ref()
            .is_some_and(|s| s.job.is_some())
        {
            "notify.task_db_copy"
        } else if self.chat.session.lock().is_ok_and(|s| s.is_running()) {
            "notify.task_assistant"
        } else {
            return self.long_tasks.running();
        };
        Some(t(label))
    }

    /// Once per frame, after the drains: remember when a long task started,
    /// and on the frame everything is idle again, notify if it is worth it.
    pub(crate) fn watch_long_tasks(&mut self, ctx: &egui::Context) {
        let busy = self.long_task_label();
        if self.long_task.is_none() {
            if let Some(label) = busy {
                self.long_task = Some((label, Instant::now()));
            }
            return;
        }
        if busy.is_some() {
            // Keep frames coming while something runs, so the end is seen
            // even when no spinner is on screen to request repaints.
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        }
        let Some((label, started)) = self.long_task.take() else {
            return;
        };
        let elapsed = started.elapsed();
        let focused = ctx.input(|i| i.viewport().focused);
        if !octa::notify::should_notify(
            self.settings.notify_on_finish,
            self.settings.notify_after_secs,
            elapsed,
            focused,
            self.settings.notify_when_focused,
        ) {
            return;
        }
        // The zip / installer copy needs its Windows identity before its first
        // notification. Ticking the switch in Settings registers it, but a
        // switch that is on by default never gets ticked.
        static IDENTITY: std::sync::Once = std::sync::Once::new();
        IDENTITY
            .call_once(|| octa::notify::register_windows_identity(self.resolved_icon.svg_source()));
        let mut body = octa::i18n::t("notify.finished")
            .replace("{task}", &label)
            .replace("{elapsed}", &octa::notify::format_elapsed(elapsed));
        // The status message a drain set when the task ended says whether it
        // worked. Only one newer than the task counts.
        if let Some((msg, _)) = self
            .status_message
            .as_ref()
            .filter(|(_, at)| *at >= started)
        {
            body.push('\n');
            body.push_str(msg);
        }
        octa::notify::send("Octa".to_string(), body);
    }
}

#[cfg(test)]
#[path = "notify_watch_tests.rs"]
mod tests;
