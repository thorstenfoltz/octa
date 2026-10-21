//! Save As of a large-file tab: the whole file, or the rows its filters keep,
//! in the order on screen, written by DuckDB's `COPY` on a worker. The tab
//! holds only the page on screen, so writing its table would have given the
//! new file 2,000 rows of a file of millions.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use octa::formats::large::ExportFormat;
use octa::i18n::t;

use crate::app::pushdown::{ServerTask, spawn_local_fallible};
use crate::app::state::{OctaApp, SaveMode};

/// A large tab's Save As being written.
pub(crate) struct LargeExport {
    pub(crate) path: PathBuf,
    /// Open the written file in a new tab once it is complete.
    open: bool,
    task: ServerTask<usize>,
    interrupt: Arc<duckdb::InterruptHandle>,
}

impl LargeExport {
    pub(crate) fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Drop the job, stopping its query, and keep the flag its worker raises
    /// once it has removed its temp file, for waiting on it at exit.
    pub(crate) fn task_load(self) -> Arc<std::sync::atomic::AtomicBool> {
        self.task.load.finished.clone()
    }
}

impl Drop for LargeExport {
    /// The tab closed or Octa is exiting: stop the query, which leaves the
    /// target as it was and removes the temp file.
    fn drop(&mut self) {
        self.interrupt.interrupt();
    }
}

impl OctaApp {
    /// Write large tab `tab_idx` to `path`. A copy always: the tab stays on
    /// its own file, and `open` (or a whole-file Save As that would have
    /// moved the tab) opens the new file in a tab of its own.
    pub(crate) fn start_large_export(&mut self, tab_idx: usize, path: PathBuf, mode: SaveMode) {
        let say = |msg: String| Some((msg, std::time::Instant::now()));
        let tab = &mut self.tabs[tab_idx];
        let (filtered, open) = match mode {
            SaveMode::Rebase => (false, true),
            SaveMode::Copy { filtered, open } => (filtered, open),
        };
        // The scan reads the source while the copy is written: never over it.
        let same = |a: &std::path::Path, b: &std::path::Path| {
            a == b
                || matches!(
                    (a.canonicalize(), b.canonicalize()),
                    (Ok(x), Ok(y)) if x == y
                )
        };
        if tab
            .table
            .source_path
            .as_deref()
            .is_some_and(|src| same(std::path::Path::new(src), &path))
        {
            tab.csv_save_choice = None;
            self.status_message = say(t("largefile.save_over_source"));
            return;
        }
        let delimited = super::save::delimited_target(&path).map(|format_delimiter| {
            let choice = tab.csv_save_choice.take();
            let (csv, delimiter) =
                tab.delimited_options(&self.settings.write_options.csv, choice, format_delimiter);
            (delimiter, csv.decimal_comma, csv.bom)
        });
        let Some(format) =
            ExportFormat::for_path(&path, delimited, &self.settings.write_options.parquet)
        else {
            self.status_message = say(t("largefile.save_formats"));
            self.forget_upload(&path);
            return;
        };
        let Some(large) = tab.large.as_ref() else {
            return;
        };
        let key = tab.large_page_key.clone();
        let order = key.as_ref().and_then(|k| k.order);
        let filter = if filtered {
            key.map(|k| k.filter).unwrap_or_default()
        } else {
            String::new()
        };
        let (job, interrupt) = match large.export_job(&path, format, order, filter) {
            Ok(j) => j,
            Err(e) => {
                self.status_message = say(format!("Error saving file: {e:#}"));
                self.forget_upload(&path);
                return;
            }
        };
        self.git_marks.note_saved(&path);
        tab.large_export = Some(LargeExport {
            path,
            open,
            task: spawn_local_fallible(job),
            interrupt,
        });
    }

    /// Once per frame: land every finished large-tab Save As.
    pub(crate) fn poll_large_exports(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let Some(job) = &self.tabs[idx].large_export else {
                continue;
            };
            let Some(outcome) = job.task.poll_result() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                continue;
            };
            let Some(job) = self.tabs[idx].large_export.take() else {
                continue;
            };
            let now = std::time::Instant::now();
            match outcome {
                Ok(rows) => {
                    self.status_message = Some((
                        t("largefile.saved")
                            .replace("{rows}", &octa::ui::status_bar::format_number(rows))
                            .replace("{path}", &job.path.display().to_string()),
                        now,
                    ));
                    self.upload_saved_copy(&job.path);
                    if job.open {
                        self.load_file_in_new_tab(job.path.clone());
                    }
                }
                Err(e) => {
                    self.status_message = Some((format!("Error saving file: {e}"), now));
                    self.forget_upload(&job.path);
                }
            }
        }
    }
}
