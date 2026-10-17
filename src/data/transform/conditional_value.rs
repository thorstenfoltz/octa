//! Conditional ("CASE" / if-elseif-else) column derivation.
//!
//! Builds a new column whose value in each row is decided by the first matching
//! rule (evaluated top to bottom), falling back to an `else` value when none
//! match. The per-cell comparison reuses the conditional-formatting logic
//! ([`CondOp`] / [`rule_matches`]) so "does this value satisfy this predicate"
//! has a single source of truth across the app.
//!
//! Pure (no IO / GUI state): the GUI dialog
//! (`src/app/dialogs/conditional_column.rs`) gathers a [`CaseSpec`], this module
//! produces the column cells, and the caller materialises them through
//! [`DataTable::insert_column`] + [`DataTable::set`].

use crate::data::conditional_format::{CondOp, CondRule, rule_matches};
use crate::data::{CellValue, DataTable, MarkColor};

/// One test inside a rule: "`<cond_col>` `<op>` `<values>`".
///
/// Several values mean "any of them" (`region equals west, east`), except
/// for the negative operators, where they mean "none of them"
/// (`region does not equal west, east`): the reading a person expects from
/// the sentence, and the same as SQL's `IN` / `NOT IN`.
#[derive(Debug, Clone)]
pub struct CaseCond {
    /// Column whose value is tested. `None` means no column has been chosen
    /// yet, in which case the condition is ignored.
    pub cond_col: Option<usize>,
    pub op: CondOp,
    /// Comparison operands (ignored for `Empty` / `NotEmpty`). Blank entries
    /// are skipped unless every entry is blank, which compares against "".
    pub values: Vec<String>,
    /// Case-sensitive text comparison when `true`.
    pub case_sensitive: bool,
}

impl CaseCond {
    pub fn new() -> Self {
        Self {
            cond_col: None,
            op: CondOp::Eq,
            values: vec![String::new()],
            case_sensitive: false,
        }
    }

    /// A condition on `col` against any of `values`, case-insensitive.
    pub fn on(col: usize, op: CondOp, values: &[&str]) -> Self {
        Self {
            cond_col: Some(col),
            op,
            values: values.iter().map(|v| v.to_string()).collect(),
            case_sensitive: false,
        }
    }

    /// One predicate per value, and whether all of them must hold (the
    /// negative operators) rather than any. `None` without a column.
    fn compile(&self) -> Option<(Vec<CondRule>, bool)> {
        let column = Some(self.cond_col?);
        let filled: Vec<&String> = self
            .values
            .iter()
            .filter(|v| !v.trim().is_empty())
            .collect();
        let values: Vec<&str> = if filled.is_empty() {
            vec![""]
        } else {
            filled.into_iter().map(String::as_str).collect()
        };
        let preds = values
            .into_iter()
            .map(|value| CondRule {
                column,
                op: self.op,
                value: value.to_string(),
                color: MarkColor::Yellow,
                case_sensitive: self.case_sensitive,
            })
            .collect();
        Some((preds, matches!(self.op, CondOp::Ne | CondOp::NotContains)))
    }
}

impl Default for CaseCond {
    fn default() -> Self {
        Self::new()
    }
}

/// One branch of a conditional column: "if `<conditions>` then `output`",
/// where the conditions must all hold (`match_all`) or any one of them.
#[derive(Debug, Clone)]
pub struct CaseRule {
    pub conditions: Vec<CaseCond>,
    /// `true` = every condition must hold (and), `false` = any one (or).
    pub match_all: bool,
    /// Literal value written into the new column when this rule matches.
    pub output: String,
}

impl CaseRule {
    pub fn new() -> Self {
        Self {
            conditions: vec![CaseCond::new()],
            match_all: true,
            output: String::new(),
        }
    }

    /// Whether any condition has a column chosen; a rule without one is
    /// skipped.
    pub fn is_usable(&self) -> bool {
        self.conditions.iter().any(|c| c.cond_col.is_some())
    }
}

impl Default for CaseRule {
    fn default() -> Self {
        Self::new()
    }
}

/// A complete if / else-if / else specification for one derived column.
#[derive(Debug, Clone, Default)]
pub struct CaseSpec {
    /// Ordered rules; the first whose condition holds wins.
    pub rules: Vec<CaseRule>,
    /// Output used when no rule matches.
    pub else_output: String,
}

/// Evaluate `spec` against every row of `table`, returning the new column's
/// cells (positionally aligned with the table's rows). The first rule whose
/// condition holds decides the row's value; if none do, `else_output` is used.
/// Numeric-looking outputs become `Int` / `Float` cells; an empty output is
/// `Null`.
pub fn build_case_column(table: &DataTable, spec: &CaseSpec) -> Vec<CellValue> {
    // Compile the usable rules once, dropping conditions without a column.
    type Compiled<'a> = (Vec<(Vec<CondRule>, bool)>, bool, &'a str);
    let compiled: Vec<Compiled> = spec
        .rules
        .iter()
        .filter(|r| r.is_usable())
        .map(|r| {
            let conds = r.conditions.iter().filter_map(CaseCond::compile).collect();
            (conds, r.match_all, r.output.as_str())
        })
        .collect();

    let row_count = table.row_count();
    let mut out = Vec::with_capacity(row_count);
    for row in 0..row_count {
        let pred = |p: &CondRule| {
            let col = p.column.expect("compiled predicates always carry a column");
            let cell = table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            rule_matches(p, &cell)
        };
        let holds = |(preds, all): &(Vec<CondRule>, bool)| {
            if *all {
                preds.iter().all(pred)
            } else {
                preds.iter().any(pred)
            }
        };
        let chosen = compiled
            .iter()
            .find(|(conds, all, _)| {
                if *all {
                    conds.iter().all(holds)
                } else {
                    conds.iter().any(holds)
                }
            })
            .map(|(_, _, output)| *output);
        out.push(literal_to_cell(chosen.unwrap_or(spec.else_output.as_str())));
    }
    out
}

/// The column type that fits the produced cells: `Int64` if every value is a
/// whole number, `Float64` if numeric with decimals, else `Utf8`.
pub fn infer_case_column_type(cells: &[CellValue]) -> String {
    let mut saw_value = false;
    let mut saw_float = false;
    for cell in cells {
        match cell {
            CellValue::Int(_) => saw_value = true,
            CellValue::Float(_) => {
                saw_value = true;
                saw_float = true;
            }
            CellValue::Null => {}
            CellValue::String(s) if s.is_empty() => {}
            _ => return "Utf8".to_string(),
        }
    }
    if !saw_value {
        "Utf8".to_string()
    } else if saw_float {
        "Float64".to_string()
    } else {
        "Int64".to_string()
    }
}

/// Turn a user-typed output literal into the tightest [`CellValue`]: empty ->
/// `Null`, integer -> `Int`, decimal -> `Float`, otherwise the verbatim text.
fn literal_to_cell(s: &str) -> CellValue {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return CellValue::Null;
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        return CellValue::Int(i);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return CellValue::Float(f);
    }
    CellValue::String(s.to_string())
}

#[cfg(test)]
#[path = "conditional_value_tests.rs"]
mod tests;
