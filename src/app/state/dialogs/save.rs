//! Save-time prompts and the remaining standalone dialogs: rounding, xlsx
//! styling, schema change, workbook, batch convert, URL and time calculation.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

/// A deferred save request waiting on the user's "round on save?" decision.
/// Carries everything `do_save_tab` needs to resume once the user picks an
/// option in `round_save_prompt`.
#[derive(Debug, Clone)]
pub(crate) struct RoundSavePrompt {
    pub(crate) tab_idx: usize,
    pub(crate) path: std::path::PathBuf,
    pub(crate) save_filtered_view: bool,
}

/// A deferred `.xlsx` save waiting on the user's "carry the formatting?"
/// decision. Mirrors [`RoundSavePrompt`]: the save re-enters with the answer.
///
/// `round_decision` carries a rounding choice already made earlier in the
/// same save (the round prompt fires first, since `do_save_tab_inner` checks
/// rounding before formatting): without it, resuming this prompt would pass
/// `None` back to the round check and reopen a prompt the user already
/// answered.
#[derive(Debug, Clone)]
pub(crate) struct XlsxStylePrompt {
    pub(crate) tab_idx: usize,
    pub(crate) path: std::path::PathBuf,
    pub(crate) save_filtered_view: bool,
    pub(crate) round_decision: Option<bool>,
}

/// A deferred DB save waiting on the user's "apply schema changes?" decision.
#[derive(Debug, Clone)]
pub(crate) struct SchemaChangeSavePrompt {
    pub(crate) tab_idx: usize,
    pub(crate) path: std::path::PathBuf,
    pub(crate) save_filtered_view: bool,
    /// Human-readable lines describing the changes (added/removed columns).
    pub(crate) changes: Vec<String>,
    /// Where the backup will be written (None when backup is disabled).
    pub(crate) backup_note: Option<String>,
    /// Rounding and formatting choices already made earlier in this save (the
    /// schema check is the last of the three), threaded through for the same
    /// reason as `XlsxStylePrompt::round_decision`.
    pub(crate) round_decision: Option<bool>,
    pub(crate) style_decision: Option<bool>,
}

/// State for the "Export to PDF" dialog: how the page should look, plus a
/// cached page count so the estimate is not recomputed every frame (it samples
/// cells to size the columns).
pub(crate) struct PdfExportState {
    pub(crate) page: data::pdf_export::PageSize,
    pub(crate) landscape: bool,
    /// Print the line naming the filter and the row / column counts.
    pub(crate) include_filter: bool,
    /// `(page, landscape, visible rows, pages)` from the last count. The row
    /// count is part of the key because the dialog is not modal: filtering or
    /// switching tabs behind it must not leave a stale estimate on screen.
    pub(crate) counted: Option<(data::pdf_export::PageSize, bool, usize, usize)>,
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for PdfExportState {
    fn default() -> Self {
        Self {
            page: data::pdf_export::PageSize::default(),
            landscape: false,
            include_filter: true,
            counted: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

pub(crate) struct WorkbookState {
    /// One entry per open tab, in tab order.
    pub(crate) selected: Vec<bool>,
    pub(crate) names: Vec<String>,
    pub(crate) error: Option<String>,
}

pub(crate) struct BatchConvertState {
    pub(crate) inputs: Vec<std::path::PathBuf>,
    /// Target extension, chosen from the writable registry formats.
    pub(crate) target_ext: String,
    pub(crate) out_dir: Option<std::path::PathBuf>,
    pub(crate) overwrite: bool,
    pub(crate) size: ui::settings::DialogSize,
    /// Live progress from the worker: items finished so far.
    pub(crate) progress: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub(crate) total: usize,
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Filled by the worker when the run finishes; drained by the update loop.
    pub(crate) result:
        std::sync::Arc<std::sync::Mutex<Option<octa::data::batch_convert::BatchReport>>>,
    /// Writer options for this run, seeded from Settings when the dialog
    /// opens and editable in its Options expander. Applies to this run only.
    pub(crate) write_options: octa::formats::write_options::WriteOptions,
    /// Text buffer for the row-group size (empty = the writer's default).
    pub(crate) row_group_buf: String,
}

impl BatchConvertState {
    pub(crate) fn new(inputs: Vec<std::path::PathBuf>) -> Self {
        Self {
            inputs,
            target_ext: "parquet".to_string(),
            out_dir: None,
            overwrite: false,
            size: ui::settings::DialogSize::default(),
            progress: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            total: 0,
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            write_options: octa::formats::write_options::WriteOptions::default(),
            row_group_buf: String::new(),
        }
    }

    /// Seed the per-run write options from the user's Settings defaults.
    pub(crate) fn with_write_options(
        mut self,
        opts: octa::formats::write_options::WriteOptions,
    ) -> Self {
        self.row_group_buf = opts
            .parquet
            .row_group_size
            .map(|n| n.to_string())
            .unwrap_or_default();
        self.write_options = opts;
        self
    }
}

/// State for the Batch convert dialog (sidebar selection, or File -> Batch
/// convert...). Inputs are resolved before the dialog opens.
/// "Export workbook": which open tabs go into one .xlsx, and under what
/// sheet names. Names are seeded from the tab labels and editable, because a
/// tab label can be long, duplicated, or carry characters Excel refuses.
/// "Open URL": the address being typed, the in-flight download, and the
/// redirect question if the download ended up somewhere else.
pub(crate) struct OpenUrlState {
    pub(crate) url: String,
    pub(crate) error: Option<String>,
    pub(crate) running: bool,
    pub(crate) slot: crate::app::dialogs::open_url::UrlSlot,
    /// Set when the download was redirected and the user asked to be told.
    /// The file is already on disk; only opening it is pending.
    pub(crate) pending_redirect: Option<octa::cloud::FetchOutcome>,
}

/// Draft state for the "Save SQL snippet" dialog: the editable name and
/// description plus the captured query text.
pub(crate) struct SqlSnippetDraft {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) query: String,
}

/// Draft state for the "Save chat prompt" dialog: the editable name and
/// description plus the captured prompt body. Mirrors [`SqlSnippetDraft`].
pub(crate) struct ChatPromptDraft {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) text: String,
}

/// Which family of time calculation the dialog is configured for. Maps onto
/// the variants of [`octa::data::time_calc::TimeCalcOp`] when the user applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeCalcKind {
    Difference,
    AddSubtract,
    ConvertDuration,
    Extract,
    UnixConvert,
    ConvertTimezone,
}

/// Live state for the "Date/Time calculation" dialog. Mirrors the inputs the
/// dialog collects; on Apply it builds a `TimeCalcOp` and materialises a new
/// column (see `dialogs::time_calc`).
#[derive(Debug, Clone)]
pub(crate) struct TimeCalcDialog {
    pub(crate) kind: TimeCalcKind,
    /// Unit for Difference / AddSubtract.
    pub(crate) unit: octa::data::time_calc::TimeUnit,
    /// Source / target units for ConvertDuration.
    pub(crate) from_unit: octa::data::time_calc::TimeUnit,
    pub(crate) to_unit: octa::data::time_calc::TimeUnit,
    /// Signed amount buffer for AddSubtract.
    pub(crate) amount_buf: String,
    /// Component for Extract.
    pub(crate) component: octa::data::time_calc::DateComponent,
    /// Direction + epoch precision for UnixConvert.
    pub(crate) unix_direction: octa::data::time_calc::UnixDirection,
    pub(crate) unix_unit: octa::data::time_calc::UnixUnit,
    /// Primary input column index.
    pub(crate) col_a: usize,
    /// Second input column index (Difference only).
    pub(crate) col_b: usize,
    /// New column name buffer.
    pub(crate) new_name: String,
    /// 1-indexed insert-position buffer.
    pub(crate) insert_at_text: String,
    /// Source and target zones for ConvertTimezone. Octa datetimes carry no
    /// zone, so the source cannot be detected and has to be stated.
    pub(crate) tz_from: chrono_tz::Tz,
    pub(crate) tz_to: chrono_tz::Tz,
    /// Filter text shared by both zone pickers. There are 597 IANA zones, which
    /// is far too many to scroll through.
    pub(crate) tz_filter: String,
}
