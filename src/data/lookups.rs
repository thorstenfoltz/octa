//! Hidden lookup tables: columns that always follow another column.
//!
//! `customer_id -> customer_name, city` in a flat export means the export is
//! really two tables glued together, and a customer with two names is a data
//! bug no per-column check can see. Pure; values compare by display text, as
//! in the funnel. Single-column keys only.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::{CellValue, ColumnInfo, DataTable};

pub const DEFAULT_MIN_CONSISTENCY: f64 = 0.95;

/// A column that follows the key.
#[derive(Debug, Clone, PartialEq)]
pub struct Dependent {
    pub col: usize,
    /// Share of rows (among keys that occur more than once) that agree with
    /// their key's most common value.
    pub consistency: f64,
    /// Keys whose rows disagree on this column.
    pub conflicting_keys: usize,
    /// Rows that differ from their key's most common value.
    pub breaking_rows: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LookupFinding {
    pub key: usize,
    /// Distinct values of the key column.
    pub keys: usize,
    pub dependents: Vec<Dependent>,
}

fn cell_text(table: &DataTable, row: usize, col: usize) -> String {
    match table.get(row, col) {
        None | Some(CellValue::Null) => String::new(),
        Some(v) => v.to_string(),
    }
}

/// Each row's value as a small integer, plus the number of distinct values.
fn codes(table: &DataTable, col: usize) -> (Vec<u32>, usize) {
    let mut ids: HashMap<String, u32> = HashMap::new();
    let codes = (0..table.row_count())
        .map(|r| {
            let next = ids.len() as u32;
            *ids.entry(cell_text(table, r, col)).or_insert(next)
        })
        .collect();
    (codes, ids.len())
}

/// A dependent from its counts over the rows of repeated keys, or `None`
/// when more rows break than `min_consistency` allows. Shared with the
/// server path so both apply one budget.
pub fn dependent_from_counts(
    col: usize,
    covered: usize,
    breaking: usize,
    conflicting: usize,
    min_consistency: f64,
) -> Option<Dependent> {
    let budget = ((1.0 - min_consistency) * covered as f64).floor() as usize;
    (covered > 0 && breaking <= budget).then(|| Dependent {
        col,
        consistency: 1.0 - breaking as f64 / covered as f64,
        conflicting_keys: conflicting,
        breaking_rows: breaking,
    })
}

/// The result order: each key's dependents most consistent first, keys with
/// the most dependents first.
pub fn finish(findings: &mut [LookupFinding]) {
    for f in findings.iter_mut() {
        f.dependents.sort_by(|a, b| {
            b.consistency
                .total_cmp(&a.consistency)
                .then(a.col.cmp(&b.col))
        });
    }
    findings.sort_by(|a, b| {
        b.dependents
            .len()
            .cmp(&a.dependents.len())
            .then(a.key.cmp(&b.key))
    });
}

/// Every key column with the columns that follow it at `min_consistency`
/// or better. A key must repeat (at most half as many distinct values as
/// rows): a unique column trivially "determines" everything.
pub fn find_lookups(
    table: &DataTable,
    min_consistency: f64,
    cancel: &AtomicBool,
) -> Vec<LookupFinding> {
    let n = table.row_count();
    let cols: Vec<(Vec<u32>, usize)> = (0..table.col_count()).map(|c| codes(table, c)).collect();
    let mut findings = Vec::new();
    for (key, (key_codes, distinct)) in cols.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if *distinct < 2 || distinct * 2 > n {
            continue;
        }
        let groups = repeated_groups(key_codes);
        let covered: usize = groups.iter().map(Vec::len).sum();
        let budget = ((1.0 - min_consistency) * covered as f64).floor() as usize;
        let mut dependents = Vec::new();
        for (dep, (dep_codes, dep_distinct)) in cols.iter().enumerate() {
            if dep == key || *dep_distinct < 2 {
                continue;
            }
            if let Some(d) =
                measure(&groups, dep_codes, budget).and_then(|(breaking, conflicting)| {
                    dependent_from_counts(dep, covered, breaking, conflicting, min_consistency)
                })
            {
                dependents.push(d);
            }
        }
        if !dependents.is_empty() {
            findings.push(LookupFinding {
                key,
                keys: *distinct,
                dependents,
            });
        }
    }
    finish(&mut findings);
    findings
}

/// Row indices of every key value that occurs more than once, in order of
/// first appearance.
fn repeated_groups(key_codes: &[u32]) -> Vec<Vec<u32>> {
    let mut by_key: HashMap<u32, Vec<u32>> = HashMap::new();
    for (row, &k) in key_codes.iter().enumerate() {
        by_key.entry(k).or_default().push(row as u32);
    }
    let mut groups: Vec<Vec<u32>> = by_key.into_values().filter(|g| g.len() > 1).collect();
    groups.sort_by_key(|g| g[0]);
    groups
}

/// `(breaking_rows, conflicting_keys)`, or `None` as soon as more rows break
/// than `budget` allows. Most column pairs are unrelated and stop early.
fn measure(groups: &[Vec<u32>], dep: &[u32], budget: usize) -> Option<(usize, usize)> {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    let (mut breaking, mut conflicting) = (0, 0);
    for g in groups {
        counts.clear();
        for &r in g {
            *counts.entry(dep[r as usize]).or_default() += 1;
        }
        let top = counts.values().copied().max().unwrap_or(0);
        if top < g.len() {
            breaking += g.len() - top;
            conflicting += 1;
            if breaking > budget {
                return None;
            }
        }
    }
    Some((breaking, conflicting))
}

/// Every row of every key whose rows disagree on any of `deps`, grouped by
/// key (keys in order of first appearance, rows in file order).
pub fn breaking_rows(table: &DataTable, key: usize, deps: &[usize]) -> Vec<usize> {
    let (key_codes, _) = codes(table, key);
    let dep_codes: Vec<Vec<u32>> = deps.iter().map(|&d| codes(table, d).0).collect();
    let mut out = Vec::new();
    for g in repeated_groups(&key_codes) {
        let first = g[0] as usize;
        if dep_codes
            .iter()
            .any(|dc| g.iter().any(|&r| dc[r as usize] != dc[first]))
        {
            out.extend(g.iter().map(|&r| r as usize));
        }
    }
    out
}

/// The two tables Split out opens, plus how many keys had conflicting values
/// that the most common one settled.
pub struct Split {
    pub lookup: DataTable,
    pub main: DataTable,
    pub resolved_keys: usize,
}

pub fn split_out(table: &DataTable, key: usize, deps: &[usize]) -> Split {
    let n = table.row_count();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for r in 0..n {
        let i = *index.entry(cell_text(table, r, key)).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[i].push(r);
    }
    let mut resolved_keys = 0;
    let mut rows = Vec::with_capacity(groups.len());
    for group in &groups {
        let mut row = vec![table.get(group[0], key).cloned().unwrap_or(CellValue::Null)];
        let mut conflict = false;
        for &d in deps {
            let (value, distinct) = most_common(table, group, d);
            conflict |= distinct > 1;
            row.push(value);
        }
        resolved_keys += usize::from(conflict);
        rows.push(row);
    }
    let mut lookup = DataTable::empty();
    lookup.columns = std::iter::once(key)
        .chain(deps.iter().copied())
        .map(|c| table.columns[c].clone())
        .collect();
    lookup.rows = rows;

    let keep: Vec<usize> = (0..table.col_count())
        .filter(|c| !deps.contains(c))
        .collect();
    let mut main = DataTable::empty();
    main.columns = keep.iter().map(|&c| table.columns[c].clone()).collect();
    main.rows = (0..n)
        .map(|r| {
            keep.iter()
                .map(|&c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    Split {
        lookup,
        main,
        resolved_keys,
    }
}

/// The most common value of `col` over `rows` (ties go to the one seen
/// first) and how many distinct values there were.
fn most_common(table: &DataTable, rows: &[usize], col: usize) -> (CellValue, usize) {
    let mut seen: Vec<(String, usize, usize)> = Vec::new(); // (text, count, first row)
    for &r in rows {
        let t = cell_text(table, r, col);
        match seen.iter_mut().find(|s| s.0 == t) {
            Some(s) => s.1 += 1,
            None => seen.push((t, 1, r)),
        }
    }
    let best = seen
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)))
        .expect("a key group is never empty");
    (
        table.get(best.2, col).cloned().unwrap_or(CellValue::Null),
        seen.len(),
    )
}

/// Findings as a table for the headless surfaces: one row per dependent.
pub fn findings_table(table: &DataTable, findings: &[LookupFinding]) -> DataTable {
    let s = |v: &str| CellValue::String(v.into());
    let mut out = DataTable::empty();
    out.columns = [
        ("key", "Utf8"),
        ("follows", "Utf8"),
        ("consistency_percent", "Float64"),
        ("conflicting_keys", "Int64"),
        ("breaking_rows", "Int64"),
    ]
    .into_iter()
    .map(|(n, t)| ColumnInfo {
        name: n.into(),
        data_type: t.into(),
    })
    .collect();
    for f in findings {
        for d in &f.dependents {
            out.rows.push(vec![
                s(&table.columns[f.key].name),
                s(&table.columns[d.col].name),
                CellValue::Float((d.consistency * 1000.0).round() / 10.0),
                CellValue::Int(d.conflicting_keys as i64),
                CellValue::Int(d.breaking_rows as i64),
            ]);
        }
    }
    out
}

#[cfg(test)]
#[path = "lookups_tests.rs"]
mod tests;
