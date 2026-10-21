//! Save a big table without stopping the window: a worker writes the file
//! while the window thread hands it the rows a slice at a time. For CSV, TSV
//! and Parquet, whose writers take rows in batches, the only copies of the
//! table are the few slices in flight (a whole copy of a five-million-row
//! table is gigabytes). Every other format's writer wants the whole table,
//! so the worker puts the slices back together first ([`JobWriter::Whole`]):
//! one copy, held by the worker, while the window goes on. The copies a save
//! makes before it writes (the earlier version, a backup) are the worker's
//! too ([`PreStep`]).
//!
//! ponytail: `Whole` holds a full copy of the table while it writes. A
//! writer per format that takes rows in batches (xlsx has a constant-memory
//! mode) would remove it, format by format.

use std::collections::HashMap;

use eframe::egui;
use octa::data::num_format::NumberFormat;
use octa::data::{ColumnInfo, DataTable, MarkColor, MarkKey};
use octa::formats::SliceFormat;
use octa::formats::write_options::WriteOptions;
use octa::i18n::t;

use super::save::{SaveBack, WrittenSave};
use crate::app::pushdown::{ServerTask, spawn_local_fallible};
use crate::app::slice_feed::{RowsStamp, SliceFeed};
use crate::app::state::OctaApp;

/// How a background save writes the slices it is handed.
pub(crate) enum JobWriter {
    /// CSV, TSV or Parquet, written a slice at a time.
    Slices(SliceFormat),
    /// Any other format: the slices are put back together and the format's
    /// own writer (by registry name) writes the whole table.
    Whole {
        reader: String,
        opts: Box<WriteOptions>,
        carry: Carry,
    },
}

impl From<SliceFormat> for JobWriter {
    fn from(format: SliceFormat) -> Self {
        Self::Slices(format)
    }
}

/// What a whole-table writer reads besides the rows, which the slices do not
/// carry. A copy gets none of it, as a synchronous copy never did.
#[derive(Default)]
pub(crate) struct Carry {
    pub(crate) marks: HashMap<MarkKey, MarkColor>,
    /// Without the formulas of edited cells: there the typed value wins.
    pub(crate) formulas: HashMap<(usize, usize), String>,
    pub(crate) structural_changes: bool,
    pub(crate) source_path: Option<String>,
    pub(crate) format_name: Option<String>,
}

impl Carry {
    /// What a save of `table` itself (not a copy of it) carries.
    pub(crate) fn of(table: &DataTable) -> Self {
        Self {
            marks: table.marks.clone(),
            formulas: table
                .formulas
                .iter()
                .filter(|(k, _)| !table.edits.contains_key(k))
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
            structural_changes: table.structural_changes,
            source_path: table.source_path.clone(),
            format_name: table.format_name.clone(),
        }
    }
}

impl JobWriter {
    fn write(
        self,
        path: &std::path::Path,
        columns: Vec<ColumnInfo>,
        slices: impl Iterator<Item = anyhow::Result<DataTable>>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Slices(format) => octa::formats::write_slices(path, &format, &columns, slices),
            Self::Whole {
                reader,
                opts,
                carry,
            } => {
                let mut table = DataTable::empty();
                table.columns = columns;
                for slice in slices {
                    table.rows.append(&mut slice?.rows);
                }
                table.marks = carry.marks;
                table.formulas = carry.formulas;
                table.structural_changes = carry.structural_changes;
                table.source_path = carry.source_path;
                table.format_name = carry.format_name;
                let registry = octa::formats::FormatRegistry::new();
                let writer = registry
                    .reader_by_name(&reader)
                    .ok_or_else(|| anyhow::anyhow!("no writer for {reader}"))?;
                writer.write_file_with_options(path, &table, &opts)
            }
        }
    }
}

/// A copy a save makes of the file it is about to replace, before writing.
/// A failed one stops the save: the user asked for a way back and would not
/// have one.
pub(crate) enum PreStep {
    /// `backup_existing_file` beside the file.
    Backup(std::path::PathBuf),
    /// Keep earlier versions.
    KeepVersion {
        store: octa::file_versions::Store,
        target: std::path::PathBuf,
        limits: octa::file_versions::Limits,
    },
}

impl PreStep {
    pub(crate) fn run(&self) -> Result<(), String> {
        match self {
            Self::Backup(path) => octa::formats::backup_existing_file(path)
                .map(|_| ())
                .map_err(|e| format!("Backup failed, save aborted: {e:#}")),
            Self::KeepVersion {
                store,
                target,
                limits,
            } => octa::file_versions::keep(store, target, *limits)
                .map(|_| ())
                .map_err(|e| format!("Keeping the earlier version failed, save aborted: {e:#}")),
        }
    }
}

/// What a save does around its write: [`PreStep`]s first, then what it owes
/// a zip or a compressed original ([`SaveBack`]).
#[derive(Default)]
pub(crate) struct Steps {
    pub(crate) before: Vec<PreStep>,
    pub(crate) back: Option<SaveBack>,
}

/// What to do once a save started from the close or exit question worked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AfterSave {
    CloseTab,
    Exit,
}

/// How a save ended.
pub(crate) enum Outcome {
    /// Written; `back` is what the save owed a zip or a compressed original.
    /// `table_changed`: the tab's table changed after its last row was handed
    /// over, so the file holds the table as it was and the tab is not touched.
    Done {
        back: Option<Result<(), String>>,
        table_changed: bool,
    },
    Failed(String),
    /// The table changed before every row was handed over: stopped, and the
    /// file was not replaced.
    Changed,
}

/// A save whose rows are still being written.
pub(crate) struct SaveJob {
    pub(crate) save: WrittenSave,
    /// Upload the file to its cloud object once written (a plain Save of a
    /// cloud tab).
    pub(crate) upload: bool,
    /// Close the tab or exit once the save worked (the close question).
    pub(crate) then: Option<AfterSave>,
    /// The save backs up a file the assistant changed; the tab's flag is
    /// cleared once it worked.
    pub(crate) assistant_backup: bool,
    feed: SliceFeed,
    task: ServerTask<Option<Result<(), String>>>,
    stamp: RowsStamp,
}

impl SaveJob {
    /// Start writing `rows` of `table` (all of them when `None`), rounded by
    /// `round`, with `steps` around the write.
    pub(crate) fn start(
        table: &DataTable,
        rows: Option<Vec<usize>>,
        writer: impl Into<JobWriter>,
        round: Option<HashMap<usize, NumberFormat>>,
        steps: Steps,
        save: WrittenSave,
        slice_rows: usize,
    ) -> Self {
        let (feed, rx) = SliceFeed::new(table, rows, round, slice_rows);
        let path = save.path.clone();
        let columns = table.columns.clone();
        let writer = writer.into();
        let task = spawn_local_fallible(move || {
            for step in &steps.before {
                step.run().map_err(anyhow::Error::msg)?;
            }
            let slices = std::iter::from_fn(|| match rx.recv() {
                Ok(Some(slice)) => Some(Ok(slice)),
                Ok(None) => None,
                // The job was dropped: its tab closed or its table changed.
                Err(_) => Some(Err(anyhow::anyhow!("the save was stopped"))),
            });
            writer.write(&path, columns, slices)?;
            Ok(steps.back.map(|b| b.run(&path)))
        });
        Self {
            save,
            upload: false,
            then: None,
            assistant_backup: false,
            feed,
            task,
            stamp: RowsStamp::of(table),
        }
    }

    /// Hand the worker what it has room for; `None` while it is writing.
    pub(crate) fn step(&mut self, table: &DataTable) -> Option<Outcome> {
        let table_changed = self.stamp != RowsStamp::of(table);
        if table_changed && self.feed.tx.is_some() {
            return Some(Outcome::Changed);
        }
        if !table_changed {
            self.feed.feed(table);
        }
        Some(match self.task.poll_result()? {
            Ok(back) => Outcome::Done {
                back,
                table_changed,
            },
            Err(e) => Outcome::Failed(e),
        })
    }

    /// The file being written, by name.
    pub(crate) fn file_name(&self) -> String {
        self.save
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Whether the worker has stopped, for waiting on it at exit.
    fn worker_finished(&self) -> bool {
        self.task
            .load
            .finished
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

impl OctaApp {
    /// At exit: stop every running save and give its worker a moment to
    /// remove its temp file, which would otherwise stay beside the user's file
    /// as a hidden copy of up to the whole table. A save that already had all
    /// its rows finishes; any other leaves its file as it was.
    pub(crate) fn stop_save_jobs(&mut self) {
        let mut jobs: Vec<SaveJob> = self
            .tabs
            .iter_mut()
            .filter_map(|t| t.saving.take())
            .collect();
        for job in &mut jobs {
            job.feed.tx = None;
        }
        // Dropping a large tab's Save As interrupts its query.
        let exports: Vec<_> = self
            .tabs
            .iter_mut()
            .filter_map(|t| t.large_export.take())
            .map(|j| j.task_load())
            .collect();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while (jobs.iter().any(|j| !j.worker_finished())
            || exports
                .iter()
                .any(|f| !f.load(std::sync::atomic::Ordering::Acquire)))
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Feed every running save and land the ones that finished. Every frame.
    pub(crate) fn drive_save_jobs(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now;
        let mut close = Vec::new();
        for idx in 0..self.tabs.len() {
            let tab = &mut self.tabs[idx];
            let Some(job) = tab.saving.as_mut() else {
                continue;
            };
            let Some(outcome) = job.step(&tab.table) else {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                continue;
            };
            let job = tab.saving.take().expect("borrowed above");
            let name = job.file_name();
            match outcome {
                Outcome::Changed => {
                    self.status_message =
                        Some((t("saving.stopped").replace("{name}", &name), now()));
                    self.forget_upload(&job.save.path);
                }
                Outcome::Failed(e) => {
                    self.status_message = Some((format!("Error saving file: {e}"), now()));
                    self.forget_upload(&job.save.path);
                }
                Outcome::Done {
                    table_changed: true,
                    ..
                } => {
                    self.status_message =
                        Some((format!("Saved to {}", job.save.path.display()), now()));
                    // The file is whole; only the tab moved on meanwhile.
                    self.upload_saved_copy(&job.save.path);
                }
                Outcome::Done { back, .. } => {
                    let path = job.save.path.clone();
                    if job.assistant_backup {
                        self.tabs[idx].assistant_modified = false;
                    }
                    self.finish_table_save(idx, job.save, None, back);
                    if job.upload {
                        self.maybe_upload_cloud(idx, &path);
                    }
                    // Closing waits for the save to have worked, as it did
                    // when the close question saved while the window waited.
                    if !self.tabs[idx].is_modified() {
                        match job.then {
                            Some(AfterSave::CloseTab) => close.push(idx),
                            Some(AfterSave::Exit) => {
                                self.confirmed_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            None => {}
                        }
                    }
                }
            }
        }
        // Highest index first, so closing one does not shift the next.
        for idx in close.into_iter().rev() {
            self.close_tab(idx);
        }
    }
}

#[cfg(test)]
#[path = "save_job_tests.rs"]
mod tests;
