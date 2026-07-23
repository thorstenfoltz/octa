//! Per-dialog and per-feature auxiliary state types, split out of the state
//! module. Everything here is small session/dialog/pipeline state; the two core
//! structs ([`OctaApp`](super::OctaApp) / [`TabState`](super::TabState)) stay in
//! `mod.rs`. All types are re-exported from the parent, so call sites keep using
//! `crate::app::state::<Name>`.

use std::sync::{Arc, Mutex};

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::settings::DialogSize;

/// Snapshot of a tab that was just closed, used to power the
/// `ReopenLastClosedTab` (Ctrl+Shift+T) shortcut.
///
/// For tabs backed by a file on disk, the path is retained - reopening
/// rereads the file, which is cheaper than holding a full `TabState` clone
/// and keeps any concurrent edits visible. For scratch tabs (no source
/// path: parsed-in-new-tab, raw edits, empty welcome tab) only the textual
/// payload (`raw_content` + view mode + format label) is kept - enough to
/// recreate the visible state without trying to deep-clone egui textures,
/// commonmark caches, etc. Truly empty tabs are not snapshotted.
pub(crate) enum ClosedTabSnapshot {
    Path(std::path::PathBuf),
    Scratch {
        raw_content: String,
        view_mode: ViewMode,
        format_name: Option<String>,
    },
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

/// A named jump target within a tab. Session-only and fixed-position: a
/// bookmark points at a row (and optionally a column) index and does not track
/// later row inserts or deletes.
#[derive(Debug, Clone)]
pub(crate) struct Bookmark {
    pub name: String,
    pub row: usize,
    pub col: Option<usize>,
}

/// Draft state for the "name this bookmark" dialog.
#[derive(Clone)]
pub(crate) struct BookmarkDraft {
    pub name_buf: String,
    pub row: usize,
    pub col: Option<usize>,
    pub size: ui::settings::DialogSize,
}

/// State for the "Rename columns" dialog: an editable list of the active tab's
/// columns and its window sizing. The buffer is seeded with one column name per
/// line; the user appends `,newname` to a line to rename it and leaves the rest
/// untouched. The parsed preview is recomputed each frame from `input_buf`
/// against the active tab's column names.
#[derive(Clone, Default)]
pub(crate) struct RenameColumnsState {
    pub input_buf: String,
    pub size: ui::settings::DialogSize,
}

impl RenameColumnsState {
    /// Seed the dialog with every column of the active tab, one per line, so the
    /// user only has to append `,newname` to the columns they want to rename.
    pub(crate) fn from_columns(columns: &[String]) -> Self {
        let mut input_buf = columns.join("\n");
        if !input_buf.is_empty() {
            input_buf.push('\n');
        }
        Self {
            input_buf,
            size: ui::settings::DialogSize::default(),
        }
    }
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

/// Draft state for the "Rename tab" dialog. Renames the tab's display name only
/// (the file path is unchanged); an empty name reverts to the file name.
#[derive(Clone)]
pub(crate) struct TabRenameDraft {
    pub tab_index: usize,
    pub name_buf: String,
    pub size: ui::settings::DialogSize,
}

/// Cache entry for the SQL workspace inspector. Stores either the
/// successful introspection or the error message returned by the workspace
/// so the inspector can render the error inline instead of refetching every
/// frame.
#[derive(Debug, Clone)]
pub(crate) struct InspectorCacheEntry {
    pub(crate) result: Result<octa::sql::TableInspection, String>,
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

/// Correlation-matrix dialog state: just the method (the engine correlates over
/// every numeric column, so there is nothing else to pick).
pub(crate) struct CorrelationState {
    pub(crate) method: octa::data::correlation::CorrMethod,
    pub(crate) size: DialogSize,
}

/// Which column-shaping transform the dialog is configured for. Maps onto the
/// pure functions in [`octa::data::transform`] when the user applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransformOp {
    /// One column -> several (by delimiter / regex / fixed width).
    Split,
    /// Several columns -> one joined column.
    Merge,
    /// Fill empty cells from the value above.
    FillDown,
    /// Fill empty cells from the value below.
    FillUp,
    /// Pull a regex match from each cell into a new column.
    Extract,
    /// Find/replace within one column's cells.
    Replace,
}

impl TransformOp {
    pub(crate) const ALL: &'static [TransformOp] = &[
        TransformOp::Split,
        TransformOp::Merge,
        TransformOp::FillDown,
        TransformOp::FillUp,
        TransformOp::Extract,
        TransformOp::Replace,
    ];

    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            TransformOp::Split => "transform_op.split",
            TransformOp::Merge => "transform_op.merge",
            TransformOp::FillDown => "transform_op.fill_down",
            TransformOp::FillUp => "transform_op.fill_up",
            TransformOp::Extract => "transform_op.extract",
            TransformOp::Replace => "transform_op.replace",
        }
    }

    /// Whether this op materialises one or more *new* columns (so the dialog
    /// should offer a name + insert-position). Fill / Replace edit in place.
    pub(crate) fn creates_column(self) -> bool {
        matches!(
            self,
            TransformOp::Split | TransformOp::Merge | TransformOp::Extract
        )
    }
}

/// How [`TransformOp::Split`] divides each cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitMode {
    Delimiter,
    Regex,
    FixedWidth,
}

impl SplitMode {
    pub(crate) const ALL: &'static [SplitMode] = &[
        SplitMode::Delimiter,
        SplitMode::Regex,
        SplitMode::FixedWidth,
    ];

    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            SplitMode::Delimiter => "transform_op.split_delimiter",
            SplitMode::Regex => "transform_op.split_regex",
            SplitMode::FixedWidth => "transform_op.split_width",
        }
    }
}

/// State for the Transform-column dialog (Edit -> Transform column...).
/// App-level: the transform applies to the active tab in place. Column
/// references are indices into the active table's `columns`.
pub(crate) struct TransformState {
    pub(crate) op: TransformOp,
    /// Source column for Split / FillDown / FillUp / Extract / Replace.
    pub(crate) col: Option<usize>,
    /// Merge: the ordered list of columns to join.
    pub(crate) merge_cols: Vec<usize>,
    /// Split mode + its parameter buffers.
    pub(crate) split_mode: SplitMode,
    pub(crate) split_delim: String,
    pub(crate) split_regex: String,
    pub(crate) split_width: String,
    /// Merge separator.
    pub(crate) merge_sep: String,
    /// Extract regex pattern.
    pub(crate) extract_pattern: String,
    /// Replace: search query + mode + replacement text.
    pub(crate) replace_query: String,
    pub(crate) replace_mode: octa::data::SearchMode,
    pub(crate) replace_with: String,
    /// For column-creating ops (Split / Merge / Extract): the output column
    /// name. Empty = the op's auto default (`merged`, `<src>_extracted`,
    /// `<src>_N`). For Split it is used as the base for `<name>_N`.
    pub(crate) new_name: String,
    /// 1-based insert position buffer for the new column(s). Empty = the op's
    /// natural default (after the source column; end for Merge).
    pub(crate) insert_pos_text: String,
    /// Last error (e.g. invalid regex), shown in the dialog.
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for TransformState {
    fn default() -> Self {
        Self {
            op: TransformOp::Split,
            col: None,
            merge_cols: Vec::new(),
            split_mode: SplitMode::Delimiter,
            split_delim: ",".to_string(),
            split_regex: String::new(),
            split_width: "1".to_string(),
            merge_sep: " ".to_string(),
            extract_pattern: String::new(),
            replace_query: String::new(),
            replace_mode: octa::data::SearchMode::Plain,
            replace_with: String::new(),
            new_name: String::new(),
            insert_pos_text: String::new(),
            error: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// Live state for the "Conditional column" dialog (Edit -> Conditional
/// column...). Builds a new column from an if / else-if / else rule chain over
/// the active tab; on Apply the rules are evaluated by
/// [`octa::data::transform::build_case_column`] and materialised as a new
/// column. App-level (applies to the active tab); column references are indices
/// into the active table's `columns`.
pub(crate) struct ConditionalColumnState {
    /// Ordered if / else-if rules (first match wins).
    pub(crate) rules: Vec<octa::data::transform::CaseRule>,
    /// Output written when no rule matches (the `else` branch).
    pub(crate) else_output: String,
    /// New column name. Empty falls back to a default ("derived").
    pub(crate) new_name: String,
    /// 1-based insert position buffer. Empty = append at the end.
    pub(crate) insert_pos_text: String,
    /// Last error, shown in the dialog.
    pub(crate) error: Option<String>,
    /// Dialog window sizing (Normal / Maximized / Minimized).
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for ConditionalColumnState {
    fn default() -> Self {
        Self {
            rules: vec![octa::data::transform::CaseRule::new()],
            else_output: String::new(),
            new_name: String::new(),
            insert_pos_text: String::new(),
            error: None,
            size: ui::settings::DialogSize::default(),
        }
    }
}

/// One editable rule row in the Anonymise dialog: a target column plus the
/// chosen strategy and its parameters. Parameters for *all* strategies are
/// held at once (not an enum) so switching the strategy dropdown keeps the
/// other fields' values; the dialog reads only the active strategy's fields.
/// `column` is an index into the active table's `columns`.
#[derive(Clone)]
pub(crate) struct AnonRuleDraft {
    /// One or more source columns (indices into the active table). With Hash
    /// and two or more columns, the values are combined into one new column.
    pub(crate) columns: std::collections::BTreeSet<usize>,
    pub(crate) kind: AnonStrategyKind,
    pub(crate) hash_algo: octa::data::transform::HashAlgo,
    /// Output the full digest (default). When false, truncate to `hash_length`.
    pub(crate) hash_full: bool,
    pub(crate) hash_length: String,
    /// Name for the derived column (multi-column hash).
    pub(crate) new_column: String,
    pub(crate) keep_end: octa::data::transform::KeepEnd,
    pub(crate) mask_count: String,
    pub(crate) mask_char: String,
    /// When on, every masked output gets `mask_fixed_len` mask characters so
    /// the original length stops leaking. Off = mask exactly the hidden chars.
    pub(crate) mask_fixed_len_on: bool,
    pub(crate) mask_fixed_len: String,
    pub(crate) redact_token: String,
    pub(crate) redact_use_null: bool,
    pub(crate) fake_kind: octa::data::transform::FakeKind,
}

impl Default for AnonRuleDraft {
    fn default() -> Self {
        Self {
            columns: std::collections::BTreeSet::new(),
            kind: AnonStrategyKind::Hash,
            hash_algo: octa::data::transform::HashAlgo::Sha256,
            hash_full: true,
            hash_length: "12".to_string(),
            new_column: String::new(),
            keep_end: octa::data::transform::KeepEnd::Last,
            mask_count: "4".to_string(),
            mask_char: "*".to_string(),
            mask_fixed_len_on: false,
            mask_fixed_len: "8".to_string(),
            redact_token: "[REDACTED]".to_string(),
            redact_use_null: false,
            fake_kind: octa::data::transform::FakeKind::Name,
        }
    }
}

/// Which strategy a rule row is editing. Mirrors
/// [`octa::data::transform::AnonStrategy`]'s variants without their payloads,
/// so the dropdown can switch strategy while the draft keeps every field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnonStrategyKind {
    Hash,
    PartialMask,
    Redact,
    Fake,
}

impl AnonStrategyKind {
    pub(crate) const ALL: &'static [AnonStrategyKind] = &[
        AnonStrategyKind::Hash,
        AnonStrategyKind::PartialMask,
        AnonStrategyKind::Redact,
        AnonStrategyKind::Fake,
    ];
    pub(crate) fn label_t(self) -> String {
        match self {
            AnonStrategyKind::Hash => octa::i18n::t("anon_strategy.hash"),
            AnonStrategyKind::PartialMask => octa::i18n::t("anon_strategy.partial_mask"),
            AnonStrategyKind::Redact => octa::i18n::t("anon_strategy.redact"),
            AnonStrategyKind::Fake => octa::i18n::t("anon_strategy.fake"),
        }
    }
}

/// Whether Anonymise rewrites the active table or builds a clean copy.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnonymizeOutput {
    InPlace,
    NewColumns,
    NewTab,
}

/// Live state for the "Anonymise columns" dialog (Edit -> Anonymise
/// columns...). App-level (applies to the active tab).
pub(crate) struct AnonymizeState {
    pub(crate) rules: Vec<AnonRuleDraft>,
    pub(crate) salt: String,
    pub(crate) output: AnonymizeOutput,
    pub(crate) error: Option<String>,
    pub(crate) size: ui::settings::DialogSize,
}

impl Default for AnonymizeState {
    fn default() -> Self {
        Self {
            rules: vec![AnonRuleDraft::default()],
            salt: String::new(),
            output: AnonymizeOutput::InPlace,
            error: None,
            size: ui::settings::DialogSize::default(),
        }
    }
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

/// Which family of time calculation the dialog is configured for. Maps onto
/// the variants of [`octa::data::time_calc::TimeCalcOp`] when the user applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeCalcKind {
    Difference,
    AddSubtract,
    ConvertDuration,
    Extract,
    UnixConvert,
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
}

/// A file read running on a background thread so the UI stays responsive.
/// `finish_single_load` consumes the result when the worker completes.
pub(crate) struct PendingLoad {
    pub(crate) path: std::path::PathBuf,
    pub(crate) format_name: String,
    pub(crate) rx: std::sync::mpsc::Receiver<anyhow::Result<DataTable>>,
}

/// State for the multi-select Excel sheet picker. `selected[i]` tracks
/// whether `sheet_names[i]` is ticked; the first `excel_max_auto_sheets` are
/// pre-checked when the picker opens.
pub(crate) struct SheetPickerState {
    pub(crate) path: std::path::PathBuf,
    pub(crate) sheet_names: Vec<String>,
    pub(crate) selected: Vec<bool>,
}

/// A deferred save request waiting on the user's "round on save?" decision.
/// Carries everything `do_save_tab` needs to resume once the user picks an
/// option in `round_save_prompt`.
#[derive(Debug, Clone)]
pub(crate) struct RoundSavePrompt {
    pub(crate) tab_idx: usize,
    pub(crate) path: std::path::PathBuf,
    pub(crate) save_filtered_view: bool,
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

#[derive(Clone)]
pub(crate) enum UpdateState {
    /// No check in progress
    Idle,
    /// Checking GitHub for latest version
    Checking,
    /// A newer version is available
    Available(String),
    /// Already on the latest version
    UpToDate,
    /// Currently downloading and installing
    Updating,
    /// Linux only: the new binary has been downloaded to `tmp_path`, but the
    /// install directory is not writable by the current user. Prompt the user
    /// to elevate so we can place the binary at `install_path`.
    NeedsElevation {
        version: String,
        install_path: std::path::PathBuf,
        tmp_path: std::path::PathBuf,
    },
    /// Update completed successfully
    Updated(String),
    /// An error occurred
    Error(String),
}

/// Direction for next/previous match navigation in highlight search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavDir {
    Next,
    Prev,
}

/// Views where filtering free text or collapsing nodes is meaningless, so the
/// search always highlights in place regardless of the Filter/Highlight toggle.
pub(crate) fn view_is_text_or_tree(vm: ViewMode) -> bool {
    matches!(
        vm,
        ViewMode::Notebook
            | ViewMode::Raw
            | ViewMode::Markdown
            | ViewMode::JsonTree
            | ViewMode::YamlTree
    )
}

/// Whether the active view highlights matches (vs filtering rows): true when
/// the session mode is `Highlight` or the view is a text/tree view.
pub(crate) fn effective_highlight(vm: ViewMode, mode: data::SearchResultMode) -> bool {
    mode == data::SearchResultMode::Highlight || view_is_text_or_tree(vm)
}

/// Per-tab transient state for highlight-search navigation. `match_count` and
/// `current` are recomputed by the active view each frame; `pending_jump` is a
/// one-shot request set by the search-bar buttons / Enter keys and consumed by
/// the view that owns the matches.
#[derive(Debug, Clone, Default)]
pub(crate) struct SearchNavState {
    pub(crate) match_count: usize,
    pub(crate) current: usize,
    pub(crate) pending_jump: Option<NavDir>,
}

impl SearchNavState {
    /// Reset to the empty state (no matches, no pending jump). Called whenever
    /// the query, search mode, view mode, or file changes.
    pub(crate) fn reset(&mut self) {
        self.match_count = 0;
        self.current = 0;
        self.pending_jump = None;
    }
}
