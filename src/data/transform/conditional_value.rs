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

/// One branch of a conditional column: "if `<cond_col>` `<op>` `<value>` then
/// `output`".
#[derive(Debug, Clone)]
pub struct CaseRule {
    /// Column whose value is tested. `None` means no column has been chosen
    /// yet, in which case the rule never matches (it is skipped).
    pub cond_col: Option<usize>,
    pub op: CondOp,
    /// Comparison operand (ignored for `Empty` / `NotEmpty`).
    pub value: String,
    /// Case-sensitive text comparison when `true`.
    pub case_sensitive: bool,
    /// Literal value written into the new column when this rule matches.
    pub output: String,
}

impl CaseRule {
    pub fn new() -> Self {
        Self {
            cond_col: None,
            op: CondOp::Eq,
            value: String::new(),
            case_sensitive: false,
            output: String::new(),
        }
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
    // Compile the valid rules once (drop rules with no column chosen), pairing
    // each predicate with its output. The colour field is unused here.
    let compiled: Vec<(CondRule, &str)> = spec
        .rules
        .iter()
        .filter(|r| r.cond_col.is_some())
        .map(|r| {
            (
                CondRule {
                    column: r.cond_col,
                    op: r.op,
                    value: r.value.clone(),
                    color: MarkColor::Yellow,
                    case_sensitive: r.case_sensitive,
                },
                r.output.as_str(),
            )
        })
        .collect();

    let row_count = table.row_count();
    let mut out = Vec::with_capacity(row_count);
    for row in 0..row_count {
        let mut chosen: Option<&str> = None;
        for (cond, output) in &compiled {
            let col = cond.column.expect("compiled rules always carry a column");
            let cell = table
                .get(row, col)
                .map(|v| v.to_string())
                .unwrap_or_default();
            if rule_matches(cond, &cell) {
                chosen = Some(output);
                break;
            }
        }
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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;
    use std::collections::HashMap;

    fn table() -> DataTable {
        DataTable {
            columns: vec![
                ColumnInfo {
                    name: "amount".into(),
                    data_type: "Int64".into(),
                },
                ColumnInfo {
                    name: "region".into(),
                    data_type: "Utf8".into(),
                },
            ],
            rows: vec![
                vec![CellValue::Int(150), CellValue::String("west".into())],
                vec![CellValue::Int(60), CellValue::String("east".into())],
                vec![CellValue::Int(10), CellValue::String("west".into())],
            ],
            edits: HashMap::new(),
            source_path: None,
            format_name: None,
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    #[test]
    fn numeric_if_elseif_else() {
        let spec = CaseSpec {
            rules: vec![
                CaseRule {
                    cond_col: Some(0),
                    op: CondOp::Gt,
                    value: "100".into(),
                    case_sensitive: false,
                    output: "high".into(),
                },
                CaseRule {
                    cond_col: Some(0),
                    op: CondOp::Gt,
                    value: "50".into(),
                    case_sensitive: false,
                    output: "medium".into(),
                },
            ],
            else_output: "low".into(),
        };
        let col = build_case_column(&table(), &spec);
        assert_eq!(
            col,
            vec![
                CellValue::String("high".into()),
                CellValue::String("medium".into()),
                CellValue::String("low".into()),
            ]
        );
    }

    #[test]
    fn string_condition_and_numeric_output() {
        let spec = CaseSpec {
            rules: vec![CaseRule {
                cond_col: Some(1),
                op: CondOp::Eq,
                value: "west".into(),
                case_sensitive: false,
                output: "1".into(),
            }],
            else_output: "0".into(),
        };
        let col = build_case_column(&table(), &spec);
        assert_eq!(
            col,
            vec![CellValue::Int(1), CellValue::Int(0), CellValue::Int(1)]
        );
        assert_eq!(infer_case_column_type(&col), "Int64");
    }

    #[test]
    fn rules_without_a_column_are_skipped() {
        let spec = CaseSpec {
            rules: vec![CaseRule {
                cond_col: None,
                op: CondOp::Eq,
                value: "west".into(),
                case_sensitive: false,
                output: "x".into(),
            }],
            else_output: "fallback".into(),
        };
        let col = build_case_column(&table(), &spec);
        assert!(
            col.iter()
                .all(|c| *c == CellValue::String("fallback".into()))
        );
    }
}
