//! State for the row-level operations: dedupe, impute, outliers, PII,
//! union, join, partition and the fuzzy variants.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

/// State for the "Drop duplicate rows" dialog (Edit -> Drop duplicate rows...).
/// App-level (operates on the active tab in place). Column references are
/// indices into the active table's `columns`.
pub(crate) struct DedupeState {
    /// Which columns form the duplicate key. Stored as a sorted `Vec` so the
    /// order is stable across frames. Empty vec means "whole row" (all cols).
    pub(crate) key_cols: Vec<usize>,
    /// One bool per column: `true` = included in the key. Kept in sync with
    /// `key_cols` on every frame so the checkbox list renders without a
    /// linear search per cell.
    pub(crate) col_selected: Vec<bool>,
    /// Which occurrence to keep when removing duplicates.
    pub(crate) keep: octa::data::dedupe::KeepWhich,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

impl DedupeState {
    /// Build a fresh state seeded to include all columns of a table with
    /// `col_count` columns (the default "whole row" key).
    pub(crate) fn new_all_cols(col_count: usize) -> Self {
        Self {
            key_cols: (0..col_count).collect(),
            col_selected: vec![true; col_count],
            keep: octa::data::dedupe::KeepWhich::First,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// Whether a column is numeric by declared type or by sampled values (so
/// numbers stored as text still register). Samples up to 30 non-empty cells.
pub(crate) fn column_looks_numeric(table: &DataTable, col: usize) -> bool {
    if let Some(c) = table.columns.get(col) {
        let t = c.data_type.to_ascii_lowercase();
        if t.contains("int") || t.contains("float") || t.contains("decimal") || t.contains("double")
        {
            return true;
        }
    }
    let mut seen = 0usize;
    let mut numeric = 0usize;
    for r in 0..table.row_count() {
        match table.get(r, col) {
            Some(octa::data::CellValue::Null) | None => continue,
            Some(v) => {
                let s = v.to_string();
                let s = s.trim();
                if s.is_empty() {
                    continue;
                }
                seen += 1;
                if s.replace(',', ".").parse::<f64>().is_ok() {
                    numeric += 1;
                }
                if seen >= 30 {
                    break;
                }
            }
        }
    }
    seen > 0 && numeric * 2 >= seen
}

/// State for the "Fill missing values" dialog (Edit -> Fill missing values...).
/// App-level (operates on the active tab in place). Column reference is an
/// index into the active table's `columns`.
#[derive(Default)]
pub(crate) struct ImputeState {
    /// Which column to fill (index into active table).
    pub(crate) col: usize,
    /// Which strategy is selected (index into the six-element list used by the
    /// combo box).
    pub(crate) strategy_idx: usize,
    /// Text field for the Constant strategy.
    pub(crate) constant: String,
    /// Last error from Apply, shown inline (None = no error yet).
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

/// What Apply does with the detected outliers: paint them, or add a boolean
/// column flagging the rows that contain at least one outlier cell.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutlierOutput {
    Highlight,
    NewColumn,
}

/// State for the "Detect outliers" dialog (Analyse -> Detect outliers...).
/// App-level. The user picks numeric columns + a method; Apply either paints
/// the flagged cells into the active tab's session-only `outlier_cells` set or
/// materialises an `is_outlier` boolean column.
pub(crate) struct OutlierState {
    /// One bool per column: `true` = include in the scan.
    pub(crate) col_selected: Vec<bool>,
    /// IQR or Z-score.
    pub(crate) method: octa::data::outliers::OutlierMethod,
    /// Whether Apply highlights cells or adds an `is_outlier` column.
    pub(crate) output: OutlierOutput,
    /// `k` factor as a text buffer (IQR fence multiplier / Z-score threshold).
    /// Comma-tolerant so European decimal commas parse.
    pub(crate) k_buf: String,
    /// Last error from Apply (e.g. unparseable `k`), shown inline.
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

impl OutlierState {
    /// Seed with every numeric column ticked. A column counts as numeric if its
    /// declared type is numeric **or** its sampled values parse as numbers (so
    /// numbers stored as text - common in CSVs - are still pre-selected; the
    /// engine reads them fine).
    pub(crate) fn for_table(table: &DataTable) -> Self {
        let col_selected = (0..table.col_count())
            .map(|c| column_looks_numeric(table, c))
            .collect();
        Self {
            col_selected,
            method: octa::data::outliers::OutlierMethod::Iqr,
            output: OutlierOutput::Highlight,
            k_buf: "1.5".to_string(),
            error: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// State for the "Detect PII" dialog (Analyse -> Detect PII...). Read-only
/// report of likely personal-data columns; a button hands the findings to the
/// Anonymise dialog. App-level.
pub(crate) struct PiiState {
    /// Scan results (column index + kind + confidence), computed once on open.
    pub(crate) findings: Vec<octa::data::pii::ColumnPii>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

/// Live state for the "Union tables" dialog (Analyse -> Union tables...).
/// The user picks which open tabs to stack then reviews a reconciliation plan
/// (keep checkboxes + target type per merged column) before applying.
/// Applying runs [`octa::data::union::union_tables`] and opens the result in
/// a new tab. App-level (source data spans multiple tabs).
pub(crate) struct UnionState {
    /// One bool per open tab (parallel to `app.tabs` at the time the dialog
    /// was opened): `true` = include this tab in the union. Empty in file mode.
    pub(crate) selected_tabs: Vec<bool>,
    /// Reconciliation plan: which output columns to keep and at what type.
    /// Recomputed from scratch whenever the tab selection changes.
    pub(crate) plan: octa::data::union::UnionPlan,
    /// Last error from Apply, shown inline (None = no error yet).
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
    /// **File mode.** When non-empty the union runs over these files, read from
    /// disk, instead of over open tabs. Populated by "Union selected files..."
    /// in the directory sidebar, so files can be unioned without opening a tab
    /// per file. `file_sources` / `file_tables` / `file_selected` are parallel.
    pub(crate) file_sources: Vec<std::path::PathBuf>,
    /// Tables read once from `file_sources` when the dialog opened.
    pub(crate) file_tables: Vec<octa::data::DataTable>,
    /// Per-file "include in the union" checkbox.
    pub(crate) file_selected: Vec<bool>,
    /// Fold column-name case when reconciling, so `Amount` and `amount` become
    /// one column. Off by default: differing case is a real difference to some
    /// downstream tools.
    pub(crate) ignore_case: bool,
}

/// Live state for the "Partition by column" dialog (Analyse -> Partition by
/// column...). The user picks a column of the active tab, an output directory,
/// and an optional format override; Apply writes one file per distinct value
/// into that directory (see `src/app/dialogs/partition.rs`).
pub(crate) struct PartitionState {
    /// Index of the column to partition on.
    pub(crate) col: usize,
    /// Output directory chosen by the folder picker.
    pub(crate) out_dir: Option<std::path::PathBuf>,
    /// Extension override (e.g. `"csv"`). Empty = use the source file's own
    /// extension.
    pub(crate) format: String,
    /// Flat files or Hive `col=value/` directories.
    pub(crate) layout: octa::data::partition::PartitionLayout,
    /// Last inline error from Apply.
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

/// One join condition draft: `left.left_col <op> right.right_col`. Columns are
/// indices into the chosen left / right tabs' schemas (resolved to names on
/// Apply).
pub(crate) struct JoinCondDraft {
    pub(crate) left_col: usize,
    pub(crate) op: octa::data::join::JoinOp,
    pub(crate) right_col: usize,
}

/// One join step in the Fuzzy join dialog: which table to bring in and how to
/// compare it against everything joined so far.
pub(crate) struct FuzzyStepDraft {
    /// Index into `OctaApp.tabs` of the table this step joins in.
    pub(crate) right_tab: usize,
    /// Column pairs to compare, `(left index, right index)`. `None` until the
    /// user picks a side.
    pub(crate) pairs: Vec<(Option<usize>, Option<usize>)>,
    pub(crate) method: octa::data::fuzzy_duplicates::SimilarityMethod,
    /// Comma-tolerant text buffer, like the other numeric inputs.
    pub(crate) threshold_text: String,
    /// Exact-match blocking columns. `None` on either side means no blocking.
    pub(crate) block: (Option<usize>, Option<usize>),
    pub(crate) join_type: octa::data::join::JoinType,
    pub(crate) max_rows_text: String,
}

impl FuzzyStepDraft {
    pub(crate) fn new(right_tab: usize) -> Self {
        Self {
            right_tab,
            pairs: vec![(None, None)],
            method: octa::data::fuzzy_duplicates::SimilarityMethod::default(),
            threshold_text: "0.85".to_string(),
            block: (None, None),
            join_type: octa::data::join::JoinType::Left,
            max_rows_text: "20000".to_string(),
        }
    }
}

/// The Fuzzy join dialog. The join runs on a worker thread with a polled slot:
/// without a blocking column the comparison is quadratic.
pub(crate) struct FuzzyJoinState {
    /// Index into `OctaApp.tabs` of the left (driving) table.
    pub(crate) left_tab: usize,
    /// One per join step, folded left to right.
    pub(crate) steps: Vec<FuzzyStepDraft>,
    pub(crate) running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) result: FuzzyJoinResultSlot,
    pub(crate) size: ui::settings::DialogSize,
}

/// Shared slot the fuzzy-join worker writes its outcome into.
pub(crate) type FuzzyJoinResultSlot = std::sync::Arc<
    std::sync::Mutex<Option<Result<octa::data::fuzzy_join::FuzzyJoinResult, String>>>,
>;

impl FuzzyJoinState {
    pub(crate) fn new(left_tab: usize, right_tab: usize) -> Self {
        Self {
            left_tab,
            steps: vec![FuzzyStepDraft::new(right_tab)],
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// Live state for the "Join tables" dialog (Analyse -> Join tables...).
/// The user picks a left tab and a right tab, then one or more join conditions
/// pairing any column of each side with a comparison operator (the column
/// names and types need not match - both sides are cast to a common type).
/// Applying runs [`octa::data::join::join_two`] and opens the result in a new
/// tab. App-level (source data spans two tabs).
pub(crate) struct JoinState {
    /// Index of the left (driving) tab.
    pub(crate) left_tab: usize,
    /// Index of the right tab.
    pub(crate) right_tab: usize,
    /// One or more conditions, ANDed together.
    pub(crate) conds: Vec<JoinCondDraft>,
    /// How unmatched rows are handled.
    pub(crate) join_type: octa::data::join::JoinType,
    /// Last error from Apply, shown inline (None = no error yet).
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

/// Whether the fuzzy-duplicate finder highlights rows in place or opens a
/// Live state for the "Find near-duplicates" dialog (Search -> Find
/// near-duplicates...). The scan runs on a background thread (the comparison is
/// O(n^2) within a block); the worker writes its [`FuzzyResult`] into `result`
/// and flips `running`, mirroring the multi-search panel's worker pattern.
/// App-level (operates on the active tab).
pub(crate) struct FuzzyDuplicatesState {
    pub(crate) key_cols: std::collections::BTreeSet<usize>,
    pub(crate) method: octa::data::fuzzy_duplicates::SimilarityMethod,
    /// Threshold as a percentage (0..=100) for the slider; divided by 100 when
    /// building the config.
    pub(crate) threshold_pct: f64,
    pub(crate) normalize: octa::data::fuzzy_duplicates::NormalizeOpts,
    pub(crate) block_col: Option<usize>,
    /// Row-cap text buffer (comma-tolerant), default "20000".
    pub(crate) max_rows_text: String,
    /// Output options (any combination, at least one required).
    pub(crate) out_cluster_col: bool,
    pub(crate) out_highlight: bool,
    pub(crate) out_new_tab: bool,
    /// Rows the previous run highlighted, cleared before the next highlight so
    /// re-running does not accumulate marks across the whole table.
    pub(crate) last_highlight_rows: Vec<usize>,
    /// Worker output (None until a scan completes).
    pub(crate) result: Arc<Mutex<Option<octa::data::fuzzy_duplicates::FuzzyResult>>>,
    pub(crate) running: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cancel: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) handle: Option<std::thread::JoinHandle<()>>,
    /// True once a scan has finished and its output has been applied (so the
    /// per-frame poll applies it exactly once).
    pub(crate) applied: bool,
    pub(crate) error: Option<String>,
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for FuzzyDuplicatesState {
    fn default() -> Self {
        Self {
            key_cols: std::collections::BTreeSet::new(),
            method: octa::data::fuzzy_duplicates::SimilarityMethod::default(),
            threshold_pct: 85.0,
            normalize: octa::data::fuzzy_duplicates::NormalizeOpts::default(),
            block_col: None,
            max_rows_text: "20000".to_string(),
            out_cluster_col: true,
            out_highlight: false,
            out_new_tab: false,
            last_highlight_rows: Vec::new(),
            result: Arc::new(Mutex::new(None)),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            handle: None,
            applied: false,
            error: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// What to do with duplicate rows once `find_duplicate_rows` has
/// returned them. `Highlight` marks each row in orange so the user can
/// see them in place; `NewTab` opens a new tab containing only those
/// rows, leaving the original untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FindDuplicatesMode {
    #[default]
    Highlight,
    NewTab,
}

/// State for the "Random sample" dialog: the requested row count (text buffer)
/// and window sizing. Apply builds a detached tab of N random rows.
#[derive(Clone)]
pub(crate) struct RandomSampleState {
    pub n_buf: String,
    pub size: ui::settings::DialogSize,
}

impl Default for RandomSampleState {
    fn default() -> Self {
        Self {
            n_buf: "100".to_string(),
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// State for the "Tidy up" dialog: which clean-up passes to run on the active
/// table. Apply runs the chosen passes as one undoable step.
#[derive(Clone)]
pub(crate) struct TidyUpState {
    /// Trim leading/trailing whitespace from string cells and column titles.
    pub trim: bool,
    /// Convert column names to snake_case.
    pub headers: bool,
    pub size: ui::settings::DialogSize,
}

impl Default for TidyUpState {
    fn default() -> Self {
        Self {
            trim: true,
            headers: false,
            size: ui::settings::DialogSize::default(),
        }
    }
}
