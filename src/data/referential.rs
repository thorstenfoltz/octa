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
mod tests {
    use super::*;

    fn table(name: &str, values: &[Option<&str>]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: name.to_string(),
            data_type: "Utf8".to_string(),
        }];
        t.rows = values
            .iter()
            .map(|v| {
                vec![match v {
                    Some(s) => CellValue::String((*s).to_string()),
                    None => CellValue::Null,
                }]
            })
            .collect();
        t
    }

    #[test]
    fn every_child_matching_a_parent_is_clean() {
        let parent = table("id", &[Some("1"), Some("2"), Some("3")]);
        let child = table("parent_id", &[Some("1"), Some("2"), Some("2")]);
        let r = check(&parent, 0, &child, 0);
        assert!(r.is_clean());
        assert_eq!(r.checked_rows, 3);
        assert_eq!(r.parent_values, 3);
        assert!(r.orphans.is_empty());
    }

    #[test]
    fn orphans_are_named_and_counted() {
        let parent = table("id", &[Some("1"), Some("2")]);
        let child = table(
            "parent_id",
            &[Some("1"), Some("9"), Some("9"), Some("9"), Some("7")],
        );
        let r = check(&parent, 0, &child, 0);
        assert!(!r.is_clean());
        assert_eq!(r.orphan_rows, 4);
        assert_eq!(r.orphan_values, 2);
        // Most rows first.
        assert_eq!(
            r.orphans,
            vec![
                Orphan {
                    value: "9".into(),
                    rows: 3
                },
                Orphan {
                    value: "7".into(),
                    rows: 1
                },
            ]
        );
        assert!(r.sentence().contains("4 child row"));
    }

    /// The convention every relational database uses: a null foreign key means
    /// "no parent", not "a parent that went missing".
    #[test]
    fn a_null_key_is_not_an_orphan() {
        let parent = table("id", &[Some("1")]);
        let child = table("parent_id", &[Some("1"), None, None]);
        let r = check(&parent, 0, &child, 0);
        assert!(r.is_clean());
        assert_eq!(r.null_keys, 2);
        assert_eq!(r.checked_rows, 1);
        assert!(r.sentence().starts_with("No orphans"));
    }

    /// An empty string is a missing key too, not a key whose value happens to
    /// be nothing: a CSV writes an absent value that way.
    #[test]
    fn an_empty_string_counts_as_no_key() {
        let parent = table("id", &[Some("1")]);
        let child = table("parent_id", &[Some(""), Some("  ")]);
        let r = check(&parent, 0, &child, 0);
        assert_eq!(r.null_keys, 2);
        assert_eq!(r.checked_rows, 0);
    }

    /// One side out of a CSV and the other out of a database is the common
    /// case, and `1` has to match `1` across that boundary.
    #[test]
    fn a_number_matches_its_text_form() {
        let mut parent = DataTable::empty();
        parent.columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        }];
        parent.rows = vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]];
        let child = table("parent_id", &[Some("1"), Some(" 2 ")]);
        assert!(check(&parent, 0, &child, 0).is_clean());
    }

    #[test]
    fn the_list_is_capped_but_the_counts_are_not() {
        let parent = table("id", &[Some("keep")]);
        let owned: Vec<String> = (0..MAX_LISTED + 50).map(|i| format!("x{i}")).collect();
        let child = table(
            "parent_id",
            &owned.iter().map(|s| Some(s.as_str())).collect::<Vec<_>>(),
        );
        let r = check(&parent, 0, &child, 0);
        assert_eq!(r.orphan_rows, MAX_LISTED + 50, "the count stays exact");
        assert_eq!(r.orphan_values, MAX_LISTED + 50);
        assert_eq!(r.orphans.len(), MAX_LISTED, "only the list is capped");
    }

    #[test]
    fn the_report_table_lists_one_row_per_value() {
        let parent = table("id", &[Some("1")]);
        let child = table("parent_id", &[Some("9"), Some("9")]);
        let t = report_table("orders", &check(&parent, 0, &child, 0));
        assert_eq!(t.row_count(), 1);
        assert_eq!(t.get(0, 0), Some(&CellValue::String("orders".into())));
        assert_eq!(t.get(0, 1), Some(&CellValue::String("9".into())));
        assert_eq!(t.get(0, 2), Some(&CellValue::Int(2)));
    }
}
