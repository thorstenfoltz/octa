//! State for the analysis dialogs: Report, drift, Harmonise, Correlation,
//! Pivot, Time series, sorting, git compare and schema export.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

/// The Report dialog: which sections to build, whether to sample, and where
/// to write the HTML.
///
/// Like [`SchemaDriftState`], the work runs on a worker thread with a polled
/// slot: a full pass over a wide table takes seconds.
pub(crate) struct ReportState {
    pub(crate) sections: Vec<octa::data::report::ReportSection>,
    pub(crate) sample_enabled: bool,
    /// Comma-tolerant text buffer, matching the other numeric inputs.
    pub(crate) sample_rows_text: String,
    pub(crate) destination: String,
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// `Ok(path written)` or a user-facing reason.
    pub(crate) result: ReportResultSlot,
    /// Set once a build succeeds. The dialog then shows where the file went
    /// and offers to open it, rather than vanishing and leaving the user to
    /// find it: the path is the one thing they need next.
    pub(crate) done_path: Option<std::path::PathBuf>,
    pub(crate) size: ui::settings::DialogSize,
}

/// Shared slot the report worker writes its outcome into.
pub(crate) type ReportResultSlot =
    std::sync::Arc<std::sync::Mutex<Option<Result<std::path::PathBuf, String>>>>;

impl ReportState {
    pub(crate) fn new(destination: String) -> Self {
        Self {
            sections: octa::data::report::ReportSection::ALL.to_vec(),
            sample_enabled: false,
            sample_rows_text: "10000".to_string(),
            destination,
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            done_path: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// The Schema drift dialog: a folder, two options, and a worker producing a
/// report.
///
/// Modelled on [`BatchConvertState`]: a worker thread plus polled `Arc` slots,
/// because reading several hundred file footers blocks for seconds and must
/// not run on the UI thread.
pub(crate) struct SchemaDriftState {
    pub(crate) folder: String,
    pub(crate) recursive: bool,
    pub(crate) ignore_case: bool,
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Filled by the worker when the scan finishes; drained by the update loop.
    pub(crate) result: DriftResultSlot,
    pub(crate) size: ui::settings::DialogSize,
}

/// Shared slot the drift worker writes its outcome into. `Err` carries a
/// user-facing reason (not a directory, nothing readable in it).
pub(crate) type DriftResultSlot =
    std::sync::Arc<std::sync::Mutex<Option<Result<octa::data::schema_drift::DriftReport, String>>>>;

impl SchemaDriftState {
    pub(crate) fn new(folder: String) -> Self {
        Self {
            folder,
            recursive: false,
            ignore_case: false,
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// State for the Harmonise schemas dialog.
///
/// Two phases on purpose. **Plan** scans and shows what would happen, including
/// which columns get dropped; **Run** writes. Dropping a column is the only
/// lossy part of the operation, so it has to be visible before the user commits
/// rather than discovered in the report afterwards.
pub(crate) struct HarmoniseState {
    pub(crate) folder: String,
    pub(crate) out_dir: String,
    pub(crate) recursive: bool,
    pub(crate) ignore_case: bool,
    pub(crate) overwrite: bool,
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Progress as `(done, total)` while a run is in flight.
    pub(crate) progress: std::sync::Arc<std::sync::Mutex<(usize, usize)>>,
    /// The plan, once scanned. `None` until the user presses Plan.
    pub(crate) plan: Option<octa::data::harmonise::HarmonisePlan>,
    /// Filled by the run worker; drained by the update loop.
    pub(crate) result: HarmoniseResultSlot,
    /// Filled by the plan worker.
    pub(crate) plan_slot: HarmonisePlanSlot,
    pub(crate) size: ui::settings::DialogSize,
}

pub(crate) type HarmoniseResultSlot = std::sync::Arc<
    std::sync::Mutex<Option<Result<octa::data::harmonise::HarmoniseReport, String>>>,
>;

pub(crate) type HarmonisePlanSlot =
    std::sync::Arc<std::sync::Mutex<Option<Result<octa::data::harmonise::HarmonisePlan, String>>>>;

impl HarmoniseState {
    pub(crate) fn new(folder: String) -> Self {
        Self {
            folder,
            out_dir: String::new(),
            recursive: false,
            ignore_case: false,
            overwrite: false,
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: std::sync::Arc::new(std::sync::Mutex::new((0, 0))),
            plan: None,
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            plan_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// Correlation-matrix dialog state: just the method (the engine correlates over
/// every numeric column, so there is nothing else to pick).
pub(crate) struct CorrelationState {
    pub(crate) method: octa::data::correlation::CorrMethod,
    pub(crate) size: DialogSize,
}

/// What to do with a column of numbers wearing a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum UnitsAction {
    /// Change nothing. The default, so the dialog opens on "no".
    #[default]
    LeaveAsText,
    /// Add one column holding the numbers.
    AddNumber,
    /// Add the numbers and the units as two columns.
    AddNumberAndUnit,
}

/// Unit / currency split. Detection has already run; this is the question.
pub(crate) struct UnitsState {
    pub(crate) col: usize,
    pub(crate) action: UnitsAction,
    /// What `units::detect_column` found, so the dialog can say it without
    /// re-scanning every frame.
    pub(crate) detection: octa::data::units::Detection,
    /// `(original, number, unit)` for the first few rows.
    pub(crate) preview: Vec<(String, String, String)>,
    pub(crate) size: DialogSize,
}

/// Referential integrity: a parent key and the child column pointing at it.
///
/// Same two-row shape as the distribution comparison, and for the same reason:
/// the two sides may be one tab or two.
pub(crate) struct ReferentialState {
    pub(crate) parent_tab: usize,
    pub(crate) parent_col: Option<usize>,
    pub(crate) child_tab: usize,
    pub(crate) child_col: Option<usize>,
    pub(crate) size: DialogSize,
}

/// Distribution comparison: two columns, each from any open tab.
///
/// The second tab defaults to the first, because comparing two columns of one
/// table is as common as comparing one column across two files.
pub(crate) struct DistCompareState {
    pub(crate) tab_a: usize,
    pub(crate) col_a: Option<usize>,
    pub(crate) tab_b: usize,
    pub(crate) col_b: Option<usize>,
    pub(crate) size: DialogSize,
}

/// Pivot vs Unpivot (long<->wide reshape) for the Pivot dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PivotKind {
    /// Long -> wide: spread one column's distinct values into new columns,
    /// aggregating a value column.
    Pivot,
    /// Wide -> long: melt several columns into a name/value pair.
    Unpivot,
}

/// Aggregate function used by a Pivot. Re-exported from the shared
/// `octa::data::pivot` module (same enum drives the MCP `pivot` tool).
pub(crate) use octa::data::pivot::PivotAgg;

/// State for the Pivot / Unpivot dialog. Column references are indices into the
/// active table's `columns`.
pub(crate) struct PivotState {
    pub(crate) kind: PivotKind,
    /// Pivot: the column whose distinct values become new columns.
    pub(crate) on_col: Option<usize>,
    /// Pivot: the column aggregated under each new column.
    pub(crate) value_col: Option<usize>,
    pub(crate) agg: PivotAgg,
    /// Pivot: the identity columns kept as rows (empty = DuckDB infers).
    pub(crate) group_cols: Vec<usize>,
    /// Unpivot: the columns melted into name/value pairs.
    pub(crate) unpivot_cols: Vec<usize>,
    /// Unpivot: name of the generated key column (buffer).
    pub(crate) name_col: String,
    /// Unpivot: name of the generated value column (buffer).
    pub(crate) value_name: String,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
    /// Cached bounded preview of the reshape result (first rows of running the
    /// op on a capped source sample). `Ok` = preview table, `Err` = error text,
    /// `None` = not enough inputs chosen yet. Recomputed only when
    /// `preview_key` changes (see `dialogs::pivot`), never per frame.
    pub(crate) preview: Option<Result<octa::data::DataTable, String>>,
    /// Hash of the inputs the cached `preview` was computed from.
    pub(crate) preview_key: u64,
}

impl Default for PivotState {
    fn default() -> Self {
        Self {
            kind: PivotKind::Pivot,
            on_col: None,
            value_col: None,
            agg: PivotAgg::Sum,
            group_cols: Vec::new(),
            unpivot_cols: Vec::new(),
            name_col: "name".to_string(),
            value_name: "value".to_string(),
            size: ui::settings::DialogSize::default(),
            preview: None,
            preview_key: 0,
        }
    }
}

/// Which half of the Time series dialog is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeseriesKind {
    Resample,
    Rolling,
}

/// State for the Time series dialog (Analyse -> Time series...). Column
/// references are indices into the active table's `columns`.
pub(crate) struct TimeseriesState {
    pub(crate) kind: TimeseriesKind,
    /// Resample: the timestamp column bucketed.
    pub(crate) time_col: Option<usize>,
    pub(crate) value_cols: Vec<usize>,
    pub(crate) interval: octa::data::timeseries::Interval,
    pub(crate) agg: octa::data::timeseries::TimeAgg,
    pub(crate) group_cols: Vec<usize>,
    /// Rolling: the ordering column, the single value column, the frame size.
    pub(crate) order_col: Option<usize>,
    pub(crate) roll_value_col: Option<usize>,
    /// Comma-tolerant text buffer, parsed on use like every other numeric input.
    pub(crate) window_text: String,
    pub(crate) partition_cols: Vec<usize>,
    pub(crate) size: ui::settings::DialogSize,
    /// Cached bounded preview: the op run on a capped source sample. `Ok` =
    /// preview table, `Err` = error text, `None` = not enough inputs chosen.
    /// Recomputed only when `preview_key` changes, never per frame.
    pub(crate) preview: Option<Result<octa::data::DataTable, String>>,
    pub(crate) preview_key: u64,
}

impl Default for TimeseriesState {
    fn default() -> Self {
        Self {
            kind: TimeseriesKind::Resample,
            time_col: None,
            value_cols: Vec::new(),
            interval: octa::data::timeseries::Interval::Day,
            agg: octa::data::timeseries::TimeAgg::Sum,
            group_cols: Vec::new(),
            order_col: None,
            roll_value_col: None,
            window_text: "7".to_string(),
            partition_cols: Vec::new(),
            size: ui::settings::DialogSize::Normal,
            preview: None,
            preview_key: 0,
        }
    }
}

/// One sort key in the multi-column sort dialog: a column index and a
/// direction (`true` = ascending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SortKey {
    pub(crate) col: usize,
    pub(crate) ascending: bool,
}

/// State for the multi-column sort dialog. The ordered `keys` list is the sort
/// priority: the first key is primary, later keys break ties. App-level (the
/// sort applies to the active tab in place).
pub(crate) struct MultiSortState {
    pub(crate) keys: Vec<SortKey>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for MultiSortState {
    fn default() -> Self {
        Self {
            // Start with one key so the dialog is never empty.
            keys: vec![SortKey {
                col: 0,
                ascending: true,
            }],
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// Revision-picker dialog for "Compare with git version" / "Open git version".
pub(crate) struct GitCompareState {
    /// Repository root.
    pub(crate) repo_root: std::path::PathBuf,
    /// File path relative to `repo_root`, forward-slashed.
    pub(crate) relpath: String,
    /// Original file extension (no dot), for the temp file.
    pub(crate) ext: String,
    /// Recent commits touching the file (newest first).
    pub(crate) commits: Vec<octa::git::Commit>,
    /// Selected revision; defaults to "HEAD".
    pub(crate) selected_rev: String,
    /// Human label for the selected revision (combo text / status message).
    pub(crate) selected_label: String,
    pub(crate) size: DialogSize,
}

/// Open Schema Export dialog state. Carries the currently-shown
/// target so the user can switch between renderings (Postgres ↔
/// MySQL ↔ Pydantic ↔ ...) without closing the dialog, plus the
/// window-size mode. Held on `OctaApp` rather than `TabState`
/// because the dialog operates on the active tab's column list
/// rather than per-tab persistent state.
pub(crate) struct SchemaExportState {
    pub(crate) target: octa::data::schema_export::SchemaTarget,
    pub(crate) size: ui::settings::DialogSize,
}
