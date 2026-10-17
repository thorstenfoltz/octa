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
#[path = "predicate_filter_tests.rs"]
mod tests;
