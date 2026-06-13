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

use std::collections::{HashMap, HashSet};

use crate::data::DataTable;

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
        ]
    }

    /// Stable English label (the dialog shows the localized `i18n_key`).
    pub fn label(&self) -> &'static str {
        match self {
            ValidationKind::NotNull => "Not empty",
            ValidationKind::Range { .. } => "In range",
            ValidationKind::Regex(_) => "Matches pattern",
            ValidationKind::Unique => "Unique",
            ValidationKind::MaxLength(_) => "Max length",
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
        }
    }

    /// Whether two kinds are the same variant (ignoring their parameters),
    /// for selecting in the dropdown.
    pub fn same_variant(&self, other: &ValidationKind) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
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
