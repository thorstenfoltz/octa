//! Load-time prompts and banners: date and number ambiguity, trim and parse
//! warnings, file repair, sheet picking and union progress.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

/// One-shot per-file prompt shown after loading a CSV/TSV whose size is
/// likely to make column coloring or column alignment laggy. The user can
/// either keep the slow features on (we honor their choice and don't ask
/// again for this tab) or disable them just for the current file. Choice is
/// transient - never written back to `AppSettings`.
pub(crate) struct RawPerfPrompt {
    pub(crate) tab_idx: usize,
    pub(crate) file_size: u64,
    pub(crate) file_name: String,
}

/// One promoted column whose stored canonical ISO display differs from the
/// detected source format. Collected during `run_date_inference_pass` and
/// surfaced together as a single dismissible banner above the table.
/// `original_values` carries the source strings for every row (None for
/// pre-existing nulls) so dismissing the banner can revert the column back
/// to its on-disk shape.
#[derive(Debug, Clone)]
pub(crate) struct DatePromotionInfo {
    pub(crate) col_idx: usize,
    pub(crate) column_name: String,
    pub(crate) source_label: &'static str,
    pub(crate) original_values: Vec<Option<String>>,
}

/// One column promoted from text to numbers by the load-time number pass.
/// `original_values` is what Dismiss puts back.
#[derive(Debug, Clone)]
pub(crate) struct NumberPromotionInfo {
    pub(crate) col_idx: usize,
    pub(crate) column_name: String,
    pub(crate) style_label: &'static str,
    pub(crate) original_values: Vec<Option<String>>,
}

/// Aggregate set of number promotions shown as one non-modal banner.
#[derive(Debug, Clone, Default)]
pub(crate) struct NumberWarning {
    pub(crate) tab_idx: usize,
    pub(crate) entries: Vec<NumberPromotionInfo>,
}

/// A column that is numeric but readable both ways (`1,234`). Queued so the
/// user can decide; the head of `pending_number_pickers` is the live dialog.
#[derive(Debug, Clone)]
pub(crate) struct NumberAmbiguity {
    pub(crate) tab_idx: usize,
    pub(crate) col_idx: usize,
    pub(crate) col_name: String,
    pub(crate) samples: Vec<String>,
}

/// Aggregate set of date promotions to surface to the user as a single
/// non-modal banner. `None` means no banner is currently pending. Cleared
/// when the user clicks Dismiss or opens a new file.
#[derive(Debug, Clone, Default)]
pub(crate) struct DateWarning {
    pub(crate) tab_idx: usize,
    pub(crate) entries: Vec<DatePromotionInfo>,
}

/// One column that looked date-shaped but could not be promoted because some
/// values failed to parse. `samples` holds a few of the offending raw values.
#[derive(Debug, Clone)]
pub(crate) struct DateParseFailure {
    pub(crate) column_name: String,
    pub(crate) source_label: &'static str,
    pub(crate) parsed: usize,
    pub(crate) total: usize,
    pub(crate) samples: Vec<String>,
}

/// Aggregate set of near-miss date columns surfaced as a single dismissible
/// banner above the table, explaining why they were left as text. `None` when
/// no such banner is pending.
#[derive(Debug, Clone, Default)]
pub(crate) struct DateParseWarning {
    pub(crate) tab_idx: usize,
    pub(crate) entries: Vec<DateParseFailure>,
}

/// Pending whitespace-trim notice surfaced as a dismissible banner above the
/// table. Lists the columns where leading/trailing whitespace was stripped on
/// load. Set by `apply_loaded_table` when `trim_whitespace_on_load` and
/// `warn_on_whitespace_trim` are both on and at least one column changed.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrimWarning {
    pub(crate) tab_idx: usize,
    pub(crate) columns: Vec<String>,
    /// Pre-trim values for the affected titles/cells. Lets the banner's
    /// "Dismiss" button undo the trim and restore the original whitespace.
    pub(crate) undo: octa::data::trim::TrimUndo,
}

/// Pending interactive repair prompt for a malformed delimited file. Raised
/// from `load_file` only when `offer_repair_on_malformed` is on and
/// `csv_reader::analyze_delimited` found problems. The dialog
/// (`dialogs::repair_file`) offers "Repair and open" (apply `options`),
/// "Open without repair" (lossy-decode only), or "Cancel". `preview` holds the
/// first rows of the repaired result, header included, for the dialog table.
pub(crate) struct FileRepair {
    pub(crate) path: std::path::PathBuf,
    /// Reader name: "CSV" or "TSV".
    pub(crate) format_name: String,
    /// Delimiter the normal reader would use for this file.
    pub(crate) default_delimiter: u8,
    /// Human-readable issues detected (ASCII only).
    pub(crate) issues: Vec<String>,
    /// Options that would repair the file.
    pub(crate) options: octa::formats::csv_reader::ReadOptions,
    /// First rows of the repaired result (row 0 is the header).
    pub(crate) preview: Vec<Vec<String>>,
}

/// One pending date-format ambiguity dialog request: a column whose values
/// are consistent with more than one date layout (e.g. DD/MM/YYYY and
/// MM/DD/YYYY). The user picks one, or chooses to leave the column as
/// strings.
pub(crate) struct DateAmbiguity {
    pub(crate) tab_idx: usize,
    pub(crate) col_idx: usize,
    pub(crate) col_name: String,
    pub(crate) samples: Vec<String>,
    pub(crate) date_candidates: Vec<octa::data::date_infer::DateLayout>,
    pub(crate) datetime_candidates: Vec<octa::data::date_infer::DateTimeLayout>,
}

/// Quoting convention recognized by the raw CSV/TSV alignment view. Drives
/// the inline tokenizer in `format_delimited_text` so a delimiter inside a
/// quoted field doesn't split the cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RawCsvQuote {
    /// RFC 4180 default - fields may be wrapped in `"`.
    #[default]
    Double,
    /// Fields may be wrapped in `'` (some dialects).
    Single,
    /// Either `"` or `'` opens a quoted span; whichever opens it must close it.
    Both,
    /// Quote characters carry no meaning - split purely on the delimiter.
    None,
}

/// How an embedded quote inside a quoted field is escaped. Determines whether
/// `""` collapses to `"`, whether `\"` collapses to `"`, or whether the first
/// matching quote always closes the span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RawCsvEscape {
    /// RFC 4180 default - `""` inside a `"..."` span is a literal quote.
    #[default]
    Doubled,
    /// C-style `\"` (and `\\`) escape inside the quoted span.
    Backslash,
    /// No escapes - the first matching quote closes the span.
    None,
}

/// State for the multi-select Excel sheet picker. `selected[i]` tracks
/// whether `sheet_names[i]` is ticked; the first `excel_max_auto_sheets` are
/// pre-checked when the picker opens.
pub(crate) struct SheetPickerState {
    pub(crate) path: std::path::PathBuf,
    pub(crate) sheet_names: Vec<String>,
    pub(crate) selected: Vec<bool>,
}

/// Files being read for the Union dialog on a background thread, so picking 40
/// parquet parts does not freeze the window. `drive_union_prep` consumes the
/// result and opens the dialog.
pub(crate) struct UnionPrep {
    pub(crate) rx: std::sync::mpsc::Receiver<UnionReadResult>,
}

/// What the union read worker sends back: the files it managed to read, their
/// tables (index-aligned), and how many files it had to skip.
pub(crate) type UnionReadResult = (Vec<std::path::PathBuf>, Vec<DataTable>, usize);

/// Progress of a running union phase, shared with its worker thread. Drives the
/// status-bar spinner: cloud download first, then the local read, so the two
/// phases hand over without the spinner blinking out.
///
/// `total == 0` means "not known yet" - the cloud folder listing has to finish
/// before it can say how many objects there are.
pub(crate) struct UnionProgress {
    pub(crate) label: std::sync::Arc<std::sync::Mutex<String>>,
    pub(crate) done: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub(crate) total: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl UnionProgress {
    /// Install a fresh progress object for `label`, with `total` items (0 when
    /// the count is not known yet). Returns the shared handles for the worker.
    pub(crate) fn new(label: &str, total: usize) -> Self {
        use std::sync::atomic::AtomicUsize;
        Self {
            label: std::sync::Arc::new(std::sync::Mutex::new(label.to_string())),
            done: std::sync::Arc::new(AtomicUsize::new(0)),
            total: std::sync::Arc::new(AtomicUsize::new(total)),
        }
    }

    /// Status-bar hint: the label, plus "done/total" once the total is known.
    pub(crate) fn hint(&self) -> String {
        use std::sync::atomic::Ordering::Relaxed;
        let label = self
            .label
            .lock()
            .map(|l| l.clone())
            .unwrap_or_else(|_| String::new());
        let total = self.total.load(Relaxed);
        if total == 0 {
            label
        } else {
            format!("{label} {}/{total}", self.done.load(Relaxed))
        }
    }
}
