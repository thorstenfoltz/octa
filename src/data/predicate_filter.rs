//! Comparison-based row filtering: `amount > 1000`, `country = DE`.
//!
//! `TabState.column_filters` is a set of allowed values per column, which can
//! say "country is DE" but cannot say "amount over 1000". Rather than growing
//! a second comparison engine, every decision here delegates to
//! `conditional_format::rule_matches`, so the operators, the numeric coercion
//! and the case handling stay defined in exactly one place.

use crate::data::DataTable;
use crate::data::MarkColor;
use crate::data::conditional_format::{CondOp, CondRule, rule_matches};

/// One condition on one column. A tab's filters are ANDed together.
#[derive(Debug, Clone, PartialEq)]
pub struct PredicateFilter {
    pub col: usize,
    pub op: CondOp,
    pub value: String,
    pub case_sensitive: bool,
}

impl PredicateFilter {
    /// Chip label, e.g. `amount greater than 1000`. Uses the operator's
    /// localised name, the same one the conditional-formatting dialog shows.
    pub fn label(&self, table: &DataTable) -> String {
        let name = table
            .columns
            .get(self.col)
            .map(|c| c.name.as_str())
            .unwrap_or("?");
        match self.op {
            CondOp::Empty | CondOp::NotEmpty => format!("{name} {}", self.op.label_t()),
            _ => format!("{name} {} {}", self.op.label_t(), self.value),
        }
    }
}

/// Does one cell satisfy the filter?
pub fn matches(filter: &PredicateFilter, cell: &str) -> bool {
    // `CondRule` carries a colour this path never reads; reusing the struct is
    // what keeps one comparison engine. `MarkColor` has no `Default`, so pick
    // any variant.
    let rule = CondRule {
        column: Some(filter.col),
        op: filter.op,
        value: filter.value.clone(),
        color: MarkColor::Red,
        case_sensitive: filter.case_sensitive,
    };
    rule_matches(&rule, cell)
}

/// Does the row satisfy every filter? An out-of-range column never passes:
/// a filter naming a column that is gone must hide the row rather than
/// silently stop filtering.
pub fn row_passes(filters: &[PredicateFilter], table: &DataTable, row: usize) -> bool {
    filters.iter().all(|f| {
        table
            .get(row, f.col)
            .map(|cell| matches(f, &cell.to_string()))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo};

    fn f(col: usize, op: CondOp, value: &str) -> PredicateFilter {
        PredicateFilter {
            col,
            op,
            value: value.to_string(),
            case_sensitive: false,
        }
    }

    #[test]
    fn numeric_comparison_uses_numbers_not_strings() {
        assert!(matches(&f(0, CondOp::Gt, "1000"), "1500"));
        assert!(!matches(&f(0, CondOp::Gt, "1000"), "900"));
        // As strings "900" sorts after "1000"; the shared engine coerces both
        // sides to f64, so this is a numeric comparison.
        assert!(matches(&f(0, CondOp::Le, "1000"), "1000"));
    }

    #[test]
    fn text_operators_still_work() {
        assert!(matches(&f(0, CondOp::Contains, "err"), "server error"));
        assert!(matches(&f(0, CondOp::Eq, "DE"), "de"));
        let sensitive = PredicateFilter {
            case_sensitive: true,
            ..f(0, CondOp::Eq, "DE")
        };
        assert!(!matches(&sensitive, "de"));
    }

    #[test]
    fn all_filters_must_pass() {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "amount".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "country".into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::Int(1500), CellValue::String("DE".into())],
            vec![CellValue::Int(1500), CellValue::String("FR".into())],
            vec![CellValue::Int(100), CellValue::String("DE".into())],
        ];
        let filters = vec![f(0, CondOp::Gt, "1000"), f(1, CondOp::Eq, "DE")];
        assert!(row_passes(&filters, &t, 0));
        assert!(!row_passes(&filters, &t, 1));
        assert!(!row_passes(&filters, &t, 2));
        // No filters means no filtering.
        assert!(row_passes(&[], &t, 2));
    }

    #[test]
    fn out_of_range_column_never_passes() {
        let t = DataTable::empty();
        assert!(!row_passes(&[f(9, CondOp::Eq, "x")], &t, 0));
    }

    #[test]
    fn label_names_the_column_and_operator() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        }];
        let label = f(0, CondOp::Gt, "1000").label(&t);
        assert!(label.starts_with("amount "), "{label}");
        assert!(label.ends_with(" 1000"), "{label}");
        // Operators with no operand do not print a dangling value.
        let empty = f(0, CondOp::Empty, "").label(&t);
        assert!(!empty.ends_with(' '), "{empty}");
    }
}
