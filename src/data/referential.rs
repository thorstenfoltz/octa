//! Referential integrity: which child rows point at a parent that is not there?
//!
//! A join that silently drops rows is the most expensive kind of wrong, because
//! the result still looks like a table. This names the values responsible.
//!
//! Two conventions are worth stating up front, because both would otherwise
//! look like bugs:
//!
//! - **A missing key is not an orphan.** In every relational database a null
//!   foreign key means "no parent", not "a parent that vanished". Counting
//!   those would report every optional relationship as broken, so they are
//!   counted separately and named separately.
//! - **Keys are compared as trimmed text**, which is what `join_keys` and
//!   `rel_map` already do. It is what makes `Int(1)` match `String("1")`,
//!   which is the common case when one side came from a CSV and the other from
//!   a database.

use std::collections::{HashMap, HashSet};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// How many distinct offending values to list. The count of orphan rows stays
/// exact; only the list is capped, because a column with 50,000 bad values is
/// telling you one thing and it is not the list.
pub const MAX_LISTED: usize = 500;

/// One value in the child that has no match in the parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub value: String,
    /// How many child rows carry it.
    pub rows: usize,
}

/// The state of one foreign key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Distinct non-null keys on the parent side.
    pub parent_values: usize,
    /// Child rows carrying a key at all.
    pub checked_rows: usize,
    /// Child rows whose key is empty. Not orphans; see the module note.
    pub null_keys: usize,
    /// Child rows whose key has no parent.
    pub orphan_rows: usize,
    /// The offending values, most rows first, capped at [`MAX_LISTED`].
    pub orphans: Vec<Orphan>,
    /// How many distinct offending values there are in total, listed or not.
    pub orphan_values: usize,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.orphan_rows == 0
    }

    /// One line for the status bar and the headless surfaces.
    pub fn sentence(&self) -> String {
        if self.is_clean() {
            format!(
                "No orphans: all {} child rows with a key match a parent.",
                self.checked_rows
            )
        } else {
            format!(
                "{} child row(s) across {} value(s) have no parent.",
                self.orphan_rows, self.orphan_values
            )
        }
    }
}

/// Check `child.ccol` against `parent.pcol`.
pub fn check(parent: &DataTable, pcol: usize, child: &DataTable, ccol: usize) -> Report {
    let parent_keys: HashSet<String> = (0..parent.row_count())
        .filter_map(|r| key(parent.get(r, pcol)))
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut report = Report {
        parent_values: parent_keys.len(),
        ..Default::default()
    };
    for r in 0..child.row_count() {
        match key(child.get(r, ccol)) {
            None => report.null_keys += 1,
            Some(k) => {
                report.checked_rows += 1;
                if !parent_keys.contains(&k) {
                    report.orphan_rows += 1;
                    *counts.entry(k).or_insert(0) += 1;
                }
            }
        }
    }

    report.orphan_values = counts.len();
    let mut orphans: Vec<Orphan> = counts
        .into_iter()
        .map(|(value, rows)| Orphan { value, rows })
        .collect();
    // Most rows first; ties by value so the output does not reshuffle between
    // runs (a HashMap's order is not stable).
    orphans.sort_by(|a, b| b.rows.cmp(&a.rows).then_with(|| a.value.cmp(&b.value)));
    orphans.truncate(MAX_LISTED);
    report.orphans = orphans;
    report
}

/// A cell as a comparable key, or `None` when there is no key at all.
fn key(cell: Option<&CellValue>) -> Option<String> {
    match cell {
        None | Some(CellValue::Null) => None,
        Some(v) => {
            let s = v.to_string();
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        }
    }
}

/// The orphans as a table, shared by every surface so they cannot render one
/// answer three ways.
pub fn report_table(child_label: &str, report: &Report) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [("child", "Utf8"), ("key_value", "Utf8"), ("rows", "Int64")]
        .iter()
        .map(|(name, ty)| ColumnInfo {
            name: (*name).to_string(),
            data_type: (*ty).to_string(),
        })
        .collect();
    t.rows = report
        .orphans
        .iter()
        .map(|o| {
            vec![
                CellValue::String(child_label.to_string()),
                CellValue::String(o.value.clone()),
                CellValue::Int(o.rows as i64),
            ]
        })
        .collect();
    t
}

#[cfg(test)]
#[path = "referential_tests.rs"]
mod tests;
