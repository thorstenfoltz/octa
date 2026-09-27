//! Change one column's type after the file is open: preview what will
//! convert, then convert it.
//!
//! This exists because load-time problems are fixed after opening, never
//! behind a gate before it. It differs from [`DataTable::convert_column`] in
//! the one way that matters: that path is all or nothing, refusing the whole
//! column when a single value will not parse, and it reads dates only as
//! `YYYY-MM-DD`. Here a value that will not parse **keeps its original text**
//! and is reported as a problem cell, and dates go through the same
//! `date_infer` layouts the loader itself uses, so a European column
//! converts instead of being refused.
//!
//! No parser lives here. Numbers, booleans and every already-typed value go
//! through [`can_convert_value`] / [`convert_value`], the pair the rest of
//! Octa casts with; dates go through [`DateLayout`] / [`DateTimeLayout`].
//! The only judgement this module adds is picking ONE date layout for the
//! whole column, which the loader's `infer_column` will not do for a column
//! where some values fail.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::data::date_infer::{DateLayout, DateTimeLayout};
use crate::data::{CellValue, DataTable, UndoAction, can_convert_value, convert_value};

/// How many failing values a preview carries as examples. The count of
/// failures is exact; this caps only the sample, so a column with a million
/// unparseable rows still reports honestly without building a million
/// strings for a dialog that shows a handful.
pub const FAILURE_SAMPLE: usize = 50;

/// Default for [`layout_sample`]: how many values the date-layout vote reads.
pub const DEFAULT_LAYOUT_SAMPLE: usize = 10_000;

/// How many values the DATE LAYOUT vote reads, as a process-wide setting.
///
/// # What the vote is
///
/// Converting a text column to Date or Date and time has to decide which of
/// the seven layouts the column is written in (`18.09.2026` is September 18th
/// under `DD.MM.YYYY` and nothing at all under `MM/DD/YYYY`). The decision is
/// made ONCE for the whole column, by parsing candidate values under every
/// layout and taking the one that reads the most of them. Every value is then
/// classified under that winner, so this sample never changes a reported
/// count: `convertible` and `failed` always describe every loaded row.
///
/// # Why it is capped
///
/// The vote costs `7 x sample` date parses on the UI thread, in one go, on a
/// single menu click. Uncapped on a five-million-row text column that is
/// ~35 million parses, which freezes the window for seconds. 10,000 values is
/// far more than a vote needs to be decided.
///
/// # When raising it actually helps
///
/// Only when a column's first values are not representative of the rest, for
/// instance a file sorted so that every ISO-formatted row comes first and the
/// European ones follow. Then a small sample can elect the minority layout
/// and the majority of the column keeps its text. Raising the sample (or
/// lifting it entirely) fixes that at the cost of the freeze above.
///
/// Mutable at runtime via [`set_layout_sample`] so `AppSettings` can override
/// the default without the engine knowing about the settings type, exactly as
/// `formats::INITIAL_LOAD_ROWS` does. `usize::MAX` reads the whole column.
static LAYOUT_SAMPLE: AtomicUsize = AtomicUsize::new(DEFAULT_LAYOUT_SAMPLE);

/// How many values the date-layout vote currently reads.
pub fn layout_sample() -> usize {
    LAYOUT_SAMPLE.load(Ordering::Relaxed)
}

/// Update the date-layout vote's sample size. Called from `OctaApp` after
/// `AppSettings` loads and whenever the user applies a new value. Lower-bounded
/// at 1, so a corrupt setting cannot leave the vote with nothing to read (which
/// would make every date conversion fail).
pub fn set_layout_sample(n: usize) {
    LAYOUT_SAMPLE.store(n.max(1), Ordering::Relaxed);
}

/// How to treat values that will not convert.
///
/// The default is [`Mixed`](Strictness::Mixed), which converts what parses
/// and leaves the rest as text. That is what makes the feature useful on a
/// real column: one `n/a` should not cost you the other 999 numbers.
///
/// But a column of "mostly numbers" is sometimes exactly what you need to
/// NOT have, because a typed column with three stragglers still sorts and
/// sums as a typed column while quietly excluding them.
/// [`Strict`](Strictness::Strict) refuses the whole conversion instead, so
/// you find out before the column is half-changed rather than after.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strictness {
    /// Convert what parses; anything else keeps its original text.
    #[default]
    Mixed,
    /// Convert only if EVERY value converts, otherwise change nothing.
    Strict,
}

/// What the user asked the column to become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetType {
    Date,
    DateTime,
    Integer,
    Float,
    Text,
    Boolean,
}

impl TargetType {
    /// Every target, in the order a chooser should list them.
    pub const ALL: &'static [TargetType] = &[
        TargetType::Text,
        TargetType::Integer,
        TargetType::Float,
        TargetType::Boolean,
        TargetType::Date,
        TargetType::DateTime,
    ];

    /// The locale key for this target's user-facing name. Lives on the type
    /// (like `SummaryStat::i18n_key`) because both the column header's
    /// submenu in `ui` and the dialog in `app` label the same six targets and
    /// must not spell them differently.
    pub fn i18n_key(self) -> &'static str {
        match self {
            TargetType::Text => "retype.type_text",
            TargetType::Integer => "retype.type_integer",
            TargetType::Float => "retype.type_float",
            TargetType::Boolean => "retype.type_boolean",
            TargetType::Date => "retype.type_date",
            TargetType::DateTime => "retype.type_datetime",
        }
    }

    /// The `ColumnInfo.data_type` string this target writes: the same Arrow
    /// spelling the readers produce, so a re-typed column is
    /// indistinguishable from one that loaded that way.
    pub fn data_type(self) -> &'static str {
        match self {
            TargetType::Date => "Date32",
            TargetType::DateTime => "Timestamp(Microsecond, None)",
            TargetType::Integer => "Int64",
            TargetType::Float => "Float64",
            TargetType::Text => "Utf8",
            TargetType::Boolean => "Boolean",
        }
    }
}

/// What a preview found, over the rows currently loaded. On a capped table
/// that is the loaded window, not the file: the caller pairs this with
/// `DataTable::partial_note()` to say so.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetypePreview {
    /// Rows examined.
    pub total: usize,
    /// Values that will convert. Nulls and blanks are neither converted nor
    /// failed, so `convertible + failed` is `total` minus those.
    pub convertible: usize,
    /// Values that will not convert and will keep their text. Exact.
    pub failed: usize,
    /// Up to [`FAILURE_SAMPLE`] of those failures, in row order, as
    /// `(row index, the value as shown)`.
    pub failures: Vec<(usize, String)>,
}

/// What an apply did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetypeOutcome {
    pub converted: usize,
    /// Coordinates of every value that kept its text, ready to hand to
    /// `problem_nav` so the existing next-problem jump walks them.
    pub kept_as_text: Vec<(usize, usize)>,
    /// Set under [`Strictness::Strict`] when the conversion was refused:
    /// how many values would not have converted. The column is untouched.
    pub refused: Option<usize>,
}

/// The date layout chosen for a whole column, if the target needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Date(DateLayout),
    DateTime(DateTimeLayout),
    /// Not a date target, or no layout read a single value.
    None,
}

/// What becomes of one cell.
enum Outcome {
    /// Absent: a null, or a string that is blank. Neither converted nor
    /// failed; it becomes (or stays) null on a typed target.
    Blank,
    Converted(CellValue),
    /// Will not parse. Keeps its original value.
    Failed,
}

/// The candidate that reads the most values, or `None` when none reads any.
///
/// Strictly-greater, so a TIE keeps the earlier candidate: `ALL` is ordered
/// ISO, then European, then US, and an ambiguous column like `01/02/2020`
/// should read the way the loader lists first rather than the way it lists
/// last. (`Iterator::max_by_key` would hand ties to the last candidate,
/// which is the US reading.)
fn best_layout<T: Copy>(all: &[T], values: &[&str], parse: impl Fn(T, &str) -> bool) -> Option<T> {
    let mut best: Option<(usize, T)> = None;
    for &candidate in all {
        let hits = values.iter().filter(|v| parse(candidate, v)).count();
        if hits > 0 && best.is_none_or(|(most, _)| hits > most) {
            best = Some((hits, candidate));
        }
    }
    best.map(|(_, l)| l)
}

/// Pick the one layout that reads the most of this column's values.
///
/// Choosing per column rather than per cell is what stops a single stray
/// `2026-09-18` in a `DD.MM.YYYY` column from dragging its neighbours onto a
/// different calendar convention.
fn choose_layout(table: &DataTable, col: usize, target: TargetType) -> Layout {
    let values: Vec<&str> = (0..table.row_count())
        .filter_map(|row| match table.get(row, col) {
            Some(CellValue::String(s)) if !s.trim().is_empty() => Some(s.as_str()),
            _ => None,
        })
        .take(layout_sample())
        .collect();
    if values.is_empty() {
        return Layout::None;
    }
    match target {
        TargetType::Date => best_layout(DateLayout::ALL, &values, |l, v| l.parse(v).is_some())
            .map_or(Layout::None, Layout::Date),
        TargetType::DateTime => {
            best_layout(DateTimeLayout::ALL, &values, |l, v| l.parse(v).is_some())
                .map_or(Layout::None, Layout::DateTime)
        }
        _ => Layout::None,
    }
}

/// What one cell becomes. The single place the decision is made, so a
/// preview and the apply that follows it cannot disagree.
fn classify(value: &CellValue, target: TargetType, layout: Layout) -> Outcome {
    // Text is the escape hatch: every value has a text form, so nothing can
    // fail. Strings pass through untouched rather than going through
    // `convert_value`, which turns a blank into a null.
    if target == TargetType::Text {
        return match value {
            CellValue::Null => Outcome::Blank,
            CellValue::String(_) => Outcome::Converted(value.clone()),
            other => Outcome::Converted(CellValue::String(other.to_string())),
        };
    }
    match value {
        CellValue::Null => Outcome::Blank,
        CellValue::String(s) if s.trim().is_empty() => Outcome::Blank,
        CellValue::String(s) => match (target, layout) {
            (TargetType::Date, Layout::Date(l)) => l
                .parse(s)
                .map_or(Outcome::Failed, |d| Outcome::Converted(CellValue::Date(d))),
            (TargetType::DateTime, Layout::DateTime(l)) => {
                l.parse(s).map_or(Outcome::Failed, |d| {
                    Outcome::Converted(CellValue::DateTime(d))
                })
            }
            // A date target with no usable layout: nothing in the column
            // parsed, so every value keeps its text.
            (TargetType::Date | TargetType::DateTime, _) => Outcome::Failed,
            _ => direct(value, target),
        },
        other => direct(other, target),
    }
}

/// The already-typed paths and the numeric/boolean string paths, through the
/// pair every other cast in Octa uses. Gated by `can_convert_value`, so
/// `convert_value` never has to guess.
fn direct(value: &CellValue, target: TargetType) -> Outcome {
    let ty = target.data_type();
    if can_convert_value(value, ty) {
        Outcome::Converted(convert_value(value, ty))
    } else {
        Outcome::Failed
    }
}

/// Count what `target` would do to `col`, changing nothing.
pub fn preview_retype(table: &DataTable, col: usize, target: TargetType) -> RetypePreview {
    if col >= table.columns.len() {
        return RetypePreview::default();
    }
    let layout = choose_layout(table, col, target);
    let mut preview = RetypePreview {
        total: table.row_count(),
        ..Default::default()
    };
    for row in 0..table.row_count() {
        let Some(value) = table.get(row, col) else {
            continue;
        };
        match classify(value, target, layout) {
            Outcome::Blank => {}
            Outcome::Converted(_) => preview.convertible += 1,
            Outcome::Failed => {
                preview.failed += 1;
                if preview.failures.len() < FAILURE_SAMPLE {
                    preview.failures.push((row, value.to_string()));
                }
            }
        }
    }
    preview
}

/// Convert `col` to `target`. Values that will not parse keep exactly what
/// they had and come back in [`RetypeOutcome::kept_as_text`].
///
/// The whole column is one entry on the undo stack, mirroring
/// [`DataTable::convert_column`]: the entry carries a verbatim before/after
/// snapshot, which is what lets a half-converted column undo cleanly. A
/// conversion that changes nothing pushes no entry at all.
pub fn apply_retype(table: &mut DataTable, col: usize, target: TargetType) -> RetypeOutcome {
    apply_retype_with(table, col, target, Strictness::Mixed)
}

/// [`apply_retype`] with the strictness chosen explicitly.
///
/// Under [`Strictness::Strict`] a column with even one unconvertible value
/// is left completely untouched and comes back with `refused` set, so the
/// caller can say why nothing happened. Checking first and converting
/// second is what makes that an all-or-nothing promise rather than a
/// half-applied change that has to be undone.
pub fn apply_retype_with(
    table: &mut DataTable,
    col: usize,
    target: TargetType,
    strictness: Strictness,
) -> RetypeOutcome {
    let mut outcome = RetypeOutcome::default();
    if col >= table.columns.len() {
        return outcome;
    }
    if strictness == Strictness::Strict {
        let preview = preview_retype(table, col, target);
        if preview.failed > 0 {
            outcome.refused = Some(preview.failed);
            return outcome;
        }
    }
    let layout = choose_layout(table, col, target);
    let n = table.row_count();

    // Classify the EFFECTIVE value (the pending edit where there is one),
    // because that is what the user saw in the preview and what the counts
    // have to describe.
    let old_values: Vec<CellValue> = (0..n)
        .map(|row| table.get(row, col).cloned().unwrap_or(CellValue::Null))
        .collect();
    for (row, value) in old_values.iter().enumerate() {
        match classify(value, target, layout) {
            Outcome::Blank => {}
            Outcome::Converted(_) => outcome.converted += 1,
            Outcome::Failed => outcome.kept_as_text.push((row, col)),
        }
    }

    // Rewrite the rows, and any pending edits over them, each from its own
    // source: dropping an edit later must not reveal an unconverted string.
    for row in 0..n {
        if let Some(cell) = table.rows[row].get(col)
            && let Some(new) = converted_or_kept(cell, target, layout)
        {
            table.rows[row][col] = new;
        }
    }
    let edit_keys: Vec<(usize, usize)> = table
        .edits
        .keys()
        .filter(|(_, c)| *c == col)
        .copied()
        .collect();
    for key in edit_keys {
        if let Some(cell) = table.edits.get(&key)
            && let Some(new) = converted_or_kept(cell, target, layout)
        {
            table.edits.insert(key, new);
        }
    }

    let new_values: Vec<CellValue> = (0..n)
        .map(|row| table.get(row, col).cloned().unwrap_or(CellValue::Null))
        .collect();
    let old_type = table.columns[col].data_type.clone();
    let new_type = target.data_type().to_string();
    if old_values == new_values && old_type == new_type {
        return outcome;
    }
    table.columns[col].data_type = new_type.clone();
    table.structural_changes = true;
    table.undo_stack.push(UndoAction::ConvertColumn {
        col_idx: col,
        old_type,
        new_type,
        old_values,
        new_values,
    });
    table.redo_stack.clear();
    outcome
}

/// The value to write, or `None` to leave the cell exactly as it is (which
/// is how a failure keeps its text).
fn converted_or_kept(value: &CellValue, target: TargetType, layout: Layout) -> Option<CellValue> {
    match classify(value, target, layout) {
        Outcome::Converted(v) => Some(v),
        // A blank string on a typed target becomes the null it already means.
        Outcome::Blank => (!matches!(value, CellValue::Null)).then_some(CellValue::Null),
        Outcome::Failed => None,
    }
}

#[cfg(test)]
#[path = "retype_tests.rs"]
mod tests;
