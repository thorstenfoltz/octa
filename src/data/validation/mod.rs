//! Rule-based data validation: a list of [`ValidationRule`]s is evaluated
//! against a table and the cells that fail are returned as `(row, col)` pairs.
//!
//! The logic is pure and lives here so it stays testable and reusable. The
//! session-scoped rule list lives on the GUI's `TabState`; the dialog in
//! `src/app/dialogs/validation.rs` edits it, and the renderer paints failing
//! cells red (reusing the [`MarkColor`](crate::data::MarkColor) machinery, the
//! same way conditional formatting colours cells).
//!
//! One-variant-per-rule so adding a check is a drop-in (see
//! `feedback_modular_features`).

pub mod rules_file;

use std::collections::{HashMap, HashSet};

use crate::data::conditional_format::CondOp;
use crate::data::id_checks::IdKind;
use crate::data::{CellValue, DataTable};

/// A single validation check.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationKind {
    /// The cell must not be empty / null.
    NotNull,
    /// The cell must parse as a number within the (optional) bounds. A missing
    /// bound is open on that side; a non-numeric cell fails.
    Range { min: Option<f64>, max: Option<f64> },
    /// The cell text must fully match the regular expression. An invalid
    /// pattern disables the rule (no cell fails).
    Regex(String),
    /// Every value in the column must be unique; duplicated cells fail.
    Unique,
    /// The cell text must be at most this many characters.
    MaxLength(usize),
    /// The cell must be a correctly built ID of this kind (check digit,
    /// shape). Empty cells pass: emptiness is `NotNull`'s job.
    Id(IdKind),
    /// The cell must compare with another column of the same row: numbers
    /// as numbers, dates and times by time (a date as midnight), text as
    /// text with case. Different kinds fail; an empty cell on either side
    /// passes. Needs a named column (`ValidationRule.column` is `Some`).
    CompareColumn { op: CondOp, other: usize },
    /// The cell must be exactly one of these values. Empty cells pass.
    OneOf(Vec<String>),
}

impl ValidationKind {
    /// All kinds, in menu order, with placeholder parameters. Used to populate
    /// the dialog's kind dropdown.
    pub fn all() -> Vec<ValidationKind> {
        vec![
            ValidationKind::NotNull,
            ValidationKind::Range {
                min: None,
                max: None,
            },
            ValidationKind::Regex(String::new()),
            ValidationKind::Unique,
            ValidationKind::MaxLength(0),
            ValidationKind::CompareColumn {
                op: CondOp::Ge,
                other: 0,
            },
            ValidationKind::OneOf(Vec::new()),
        ]
        .into_iter()
        .chain(IdKind::ALL.map(ValidationKind::Id))
        .collect()
    }

    /// Stable English label (the dialog shows the localized `i18n_key`).
    pub fn label(&self) -> &'static str {
        match self {
            ValidationKind::NotNull => "Not empty",
            ValidationKind::Range { .. } => "In range",
            ValidationKind::Regex(_) => "Matches pattern",
            ValidationKind::Unique => "Unique",
            ValidationKind::MaxLength(_) => "Max length",
            ValidationKind::CompareColumn { .. } => "Compare with column",
            ValidationKind::OneOf(_) => "One of these values",
            ValidationKind::Id(IdKind::Iban) => "IBAN",
            ValidationKind::Id(IdKind::CardNumber) => "Card number",
            ValidationKind::Id(IdKind::Gtin) => "EAN / ISBN / UPC",
            ValidationKind::Id(IdKind::VatId) => "VAT number (EU)",
            ValidationKind::Id(IdKind::Email) => "Email address",
        }
    }

    /// i18n key for the localized kind label.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            ValidationKind::NotNull => "validation_kind.not_null",
            ValidationKind::Range { .. } => "validation_kind.range",
            ValidationKind::Regex(_) => "validation_kind.regex",
            ValidationKind::Unique => "validation_kind.unique",
            ValidationKind::MaxLength(_) => "validation_kind.max_length",
            ValidationKind::CompareColumn { .. } => "validation_kind.compare",
            ValidationKind::OneOf(_) => "validation_kind.one_of",
            ValidationKind::Id(IdKind::Iban) => "validation_kind.iban",
            ValidationKind::Id(IdKind::CardNumber) => "validation_kind.card_number",
            ValidationKind::Id(IdKind::Gtin) => "validation_kind.gtin",
            ValidationKind::Id(IdKind::VatId) => "validation_kind.vat_id",
            ValidationKind::Id(IdKind::Email) => "validation_kind.email",
        }
    }

    /// i18n key of the hover text, for the kinds whose name alone does not
    /// say what passes.
    pub fn hint_key(&self) -> Option<&'static str> {
        match self {
            ValidationKind::Id(IdKind::Iban) => Some("validation_kind.iban_hint"),
            ValidationKind::Id(IdKind::CardNumber) => Some("validation_kind.card_number_hint"),
            ValidationKind::Id(IdKind::Gtin) => Some("validation_kind.gtin_hint"),
            ValidationKind::Id(IdKind::VatId) => Some("validation_kind.vat_id_hint"),
            ValidationKind::Id(IdKind::Email) => Some("validation_kind.email_hint"),
            ValidationKind::CompareColumn { .. } => Some("validation_kind.compare_hint"),
            ValidationKind::OneOf(_) => Some("validation_kind.one_of_hint"),
            _ => None,
        }
    }

    /// Whether two kinds are the same variant (ignoring their parameters),
    /// for selecting in the dropdown.
    pub fn same_variant(&self, other: &ValidationKind) -> bool {
        match (self, other) {
            // Each ID kind is its own entry in the dropdown.
            (ValidationKind::Id(a), ValidationKind::Id(b)) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

/// A validation rule: a [`ValidationKind`] applied to one column (`Some(idx)`)
/// or every column (`None`).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationRule {
    pub column: Option<usize>,
    pub kind: ValidationKind,
}

/// Treat a cell as empty when it is null or blank after trimming.
fn is_blank(table: &DataTable, row: usize, col: usize) -> bool {
    table
        .get(row, col)
        .map(|v| v.to_string().trim().is_empty())
        .unwrap_or(true)
}

/// What a cell is, for comparing it with another: a number (typed, or text
/// that reads as a plain one), else a date or time, else text.
enum Class {
    Number(f64),
    Time(chrono::NaiveDateTime),
    Text(String),
}

fn classify(v: &CellValue) -> Class {
    match v {
        CellValue::Int(i) => Class::Number(*i as f64),
        CellValue::Float(f) => Class::Number(*f),
        _ => {
            let s = v.to_string();
            if let Ok(n) = s.trim().parse::<f64>() {
                return Class::Number(n);
            }
            if let Some((dt, _)) = crate::data::time_calc::cell_to_datetime(v) {
                return Class::Time(dt);
            }
            Class::Text(s)
        }
    }
}

/// Does `a <op> b` hold? An empty cell on either side passes: emptiness is
/// `NotNull`'s job. Two cells of different kinds never compare, so they fail.
fn compare_holds(a: Option<&CellValue>, b: Option<&CellValue>, op: CondOp) -> bool {
    let empty = |v: Option<&CellValue>| v.is_none_or(|v| v.to_string().trim().is_empty());
    if empty(a) || empty(b) {
        return true;
    }
    let (Some(a), Some(b)) = (a, b) else {
        return true;
    };
    let ord = match (classify(a), classify(b)) {
        (Class::Number(x), Class::Number(y)) => x.partial_cmp(&y),
        (Class::Time(x), Class::Time(y)) => Some(x.cmp(&y)),
        (Class::Text(x), Class::Text(y)) => Some(x.cmp(&y)),
        _ => None,
    };
    let Some(ord) = ord else {
        return false;
    };
    match op {
        CondOp::Eq => ord.is_eq(),
        CondOp::Ne => ord.is_ne(),
        CondOp::Lt => ord.is_lt(),
        CondOp::Le => ord.is_le(),
        CondOp::Gt => ord.is_gt(),
        CondOp::Ge => ord.is_ge(),
        // Never stored for this kind: the dialog and the rules file offer
        // only the six comparisons.
        _ => true,
    }
}

/// The text a report shows for a failing cell: the cell itself, or for a
/// column comparison both values (`end=2024-01-01, start=2024-02-01`), since
/// either one alone does not say what is wrong.
pub fn sample_text(table: &DataTable, rule: &ValidationRule, row: usize, col: usize) -> String {
    let text = |c: usize| table.get(row, c).map(|v| v.to_string()).unwrap_or_default();
    match &rule.kind {
        ValidationKind::CompareColumn { other, .. } => {
            let name = |c: usize| {
                table
                    .columns
                    .get(c)
                    .map(|ci| ci.name.clone())
                    .unwrap_or_default()
            };
            format!(
                "{}={}, {}={}",
                name(col),
                text(col),
                name(*other),
                text(*other)
            )
        }
        _ => text(col),
    }
}

/// The set of cells failing any rule. Edits are honoured (`DataTable::get`
/// returns the overlaid value). Each rule contributes its failing cells; the
/// union is returned.
pub fn violations(table: &DataTable, rules: &[ValidationRule]) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    let col_count = table.col_count();
    let row_count = table.row_count();
    if col_count == 0 || row_count == 0 {
        return out;
    }

    for rule in rules {
        // Columns the rule targets.
        let cols: Vec<usize> = match rule.column {
            Some(c) if c < col_count => vec![c],
            Some(_) => continue, // stale column index
            None => (0..col_count).collect(),
        };

        match &rule.kind {
            ValidationKind::NotNull => {
                for &c in &cols {
                    for r in 0..row_count {
                        if is_blank(table, r, c) {
                            out.insert((r, c));
                        }
                    }
                }
            }
            ValidationKind::Range { min, max } => {
                for &c in &cols {
                    for r in 0..row_count {
                        if is_blank(table, r, c) {
                            continue; // empty cells are a NotNull concern, not Range
                        }
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        match s.trim().parse::<f64>() {
                            Ok(n) => {
                                let below = min.is_some_and(|lo| n < lo);
                                let above = max.is_some_and(|hi| n > hi);
                                if below || above {
                                    out.insert((r, c));
                                }
                            }
                            // Non-numeric where a number is required.
                            Err(_) => {
                                out.insert((r, c));
                            }
                        }
                    }
                }
            }
            ValidationKind::Regex(pattern) => {
                // Anchor so the whole cell must match (like a format rule).
                let anchored = format!("^(?:{pattern})$");
                let Ok(re) = regex::Regex::new(&anchored) else {
                    continue; // invalid pattern disables the rule
                };
                for &c in &cols {
                    for r in 0..row_count {
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        if !re.is_match(&s) {
                            out.insert((r, c));
                        }
                    }
                }
            }
            ValidationKind::MaxLength(max) => {
                for &c in &cols {
                    for r in 0..row_count {
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        if s.chars().count() > *max {
                            out.insert((r, c));
                        }
                    }
                }
            }
            ValidationKind::Id(kind) => {
                for &c in &cols {
                    for r in 0..row_count {
                        if is_blank(table, r, c) {
                            continue;
                        }
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        if !kind.check(&s) {
                            out.insert((r, c));
                        }
                    }
                }
            }
            ValidationKind::CompareColumn { op, other } => {
                // Needs a named column, and the other one must still exist.
                let Some(c) = rule.column else { continue };
                if *other >= col_count {
                    continue;
                }
                for r in 0..row_count {
                    if !compare_holds(table.get(r, c), table.get(r, *other), *op) {
                        out.insert((r, c));
                    }
                }
            }
            ValidationKind::OneOf(values) => {
                for &c in &cols {
                    for r in 0..row_count {
                        if is_blank(table, r, c) {
                            continue;
                        }
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        if !values.contains(&s) {
                            out.insert((r, c));
                        }
                    }
                }
            }
            ValidationKind::Unique => {
                for &c in &cols {
                    // Count each non-blank value; any value seen more than once
                    // marks all its cells. Blank cells are ignored here.
                    let mut counts: HashMap<String, Vec<usize>> = HashMap::new();
                    for r in 0..row_count {
                        if is_blank(table, r, c) {
                            continue;
                        }
                        let s = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                        counts.entry(s).or_default().push(r);
                    }
                    for rows in counts.values() {
                        if rows.len() > 1 {
                            for &r in rows {
                                out.insert((r, c));
                            }
                        }
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
