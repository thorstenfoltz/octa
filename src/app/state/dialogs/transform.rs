//! State for the column-transform family: Transform, Conditional column,
//! Anonymise and Rename columns.
//!
//! One of six files split out of `state/dialogs.rs`, which held 100 top-level
//! items in 1,573 lines. Grouped by what the state belongs to rather than
//! moved next to each dialog: a third of these types have no dialog (tab
//! snapshots, load banners, background jobs), and the rest are read by both
//! their dialog and `state/mod.rs`, so scattering them would have doubled the
//! import churn for no gain. Definitions moved unchanged.

use super::*;

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
    /// Repair text decoded with the wrong character set.
    RepairEncoding,
}

impl TransformOp {
    pub(crate) const ALL: &'static [TransformOp] = &[
        TransformOp::Split,
        TransformOp::Merge,
        TransformOp::FillDown,
        TransformOp::FillUp,
        TransformOp::Extract,
        TransformOp::Replace,
        TransformOp::RepairEncoding,
    ];

    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            TransformOp::Split => "transform_op.split",
            TransformOp::Merge => "transform_op.merge",
            TransformOp::FillDown => "transform_op.fill_down",
            TransformOp::FillUp => "transform_op.fill_up",
            TransformOp::Extract => "transform_op.extract",
            TransformOp::Replace => "transform_op.replace",
            TransformOp::RepairEncoding => "transform_op.repair_encoding",
        }
    }

    /// Whether this op materialises one or more *new* columns (so the dialog
    /// should offer a name + insert-position). Fill, Replace and
    /// RepairEncoding edit in place.
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

/// State for the "Rename columns" dialog: an editable list of the active tab's
/// columns and its window sizing. The buffer is seeded with one column name per
/// line; the user appends `,newname` to a line to rename it and leaves the rest
/// untouched. The parsed preview is recomputed each frame from `input_buf`
/// against the active tab's column names.
#[derive(Clone, Default)]
pub(crate) struct RenameColumnsState {
    pub input_buf: String,
    /// Also give every repeated column name a numbered suffix on Apply. Set
    /// by the Columns -> "Fix duplicate names..." entry, which is the same
    /// dialog opened straight onto this half of it.
    pub fix_duplicates: bool,
    /// Whether `Name` and `name` count as the same name.
    pub dedupe_ignore_case: bool,
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
            fix_duplicates: false,
            dedupe_ignore_case: false,
            size: ui::settings::DialogSize::default(),
        }
    }
}
