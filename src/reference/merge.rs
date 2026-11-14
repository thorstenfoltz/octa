//! The user's merge rule (2026-11-13): a new download overwrites a stored
//! value only where it has one; anything it lacks stays. The rebase guard
//! keeps two bases from being mixed in one series.

use std::collections::{BTreeMap, BTreeSet};

use crate::data::decimal::Exact;

use super::store::{RefColumn, RefTable};

pub type Series = BTreeMap<String, Exact>;
pub type SeriesSet = BTreeMap<String, Series>;
pub type Row = Vec<Option<String>>;
pub type Groups = BTreeMap<String, Vec<Row>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Int,
}

/// Overlapping points needed before a factor counts as a rebase.
const MIN_OVERLAP: usize = 3;
/// A ratio this close to 1 is the same base.
const SAME_BASE: f64 = 0.005;
/// Ratios this close to their median agree on one factor.
const AGREE: f64 = 0.005;
/// Share of the overlapping points that must agree.
const AGREE_SHARE: f64 = 0.9;

/// Did the source put this series on a new base? True when the values both
/// downloads have differ from 1 by more than rounding and almost all of them
/// by the same factor. A single revised value, or noise, is not a rebase.
pub fn is_rebased(old: &Series, new: &Series) -> bool {
    let mut ratios: Vec<f64> = new
        .iter()
        .filter_map(|(k, n)| {
            let o = old.get(k)?.to_f64();
            (o != 0.0).then(|| n.to_f64() / o)
        })
        .filter(|r| r.is_finite())
        .collect();
    if ratios.len() < MIN_OVERLAP {
        return false;
    }
    ratios.sort_by(f64::total_cmp);
    let median = ratios[ratios.len() / 2];
    if (median - 1.0).abs() <= SAME_BASE {
        return false;
    }
    let agree = ratios
        .iter()
        .filter(|r| ((*r / median) - 1.0).abs() <= AGREE)
        .count();
    agree as f64 >= AGREE_SHARE * ratios.len() as f64
}

/// Merge per value; a rebased series is replaced whole. Returns the merged
/// set and the names of the rebased series.
pub fn merge_series(old: &SeriesSet, new: &SeriesSet) -> (SeriesSet, Vec<String>) {
    let mut out = old.clone();
    let mut rebased = Vec::new();
    for (name, fresh) in new {
        match out.get_mut(name) {
            Some(kept) if is_rebased(kept, fresh) => {
                *kept = fresh.clone();
                rebased.push(name.clone());
            }
            Some(kept) => kept.extend(fresh.iter().map(|(k, v)| (k.clone(), *v))),
            None => {
                out.insert(name.clone(), fresh.clone());
            }
        }
    }
    (out, rebased)
}

/// A key the new download lists takes the new rows; a field the new row
/// leaves empty keeps the old row's value, rows matched by `id_cols`. A key
/// the new download lacks keeps its old rows.
pub fn merge_groups(old: &Groups, new: &Groups, id_cols: &[usize]) -> Groups {
    let id = |r: &Row| -> Vec<Option<String>> {
        id_cols
            .iter()
            .map(|&c| r.get(c).cloned().flatten())
            .collect()
    };
    let mut out = old.clone();
    for (key, rows) in new {
        let before = old.get(key);
        let filled = rows
            .iter()
            .map(|r| {
                let Some(prev) = before.and_then(|b| b.iter().find(|o| id(o) == id(r))) else {
                    return r.clone();
                };
                r.iter()
                    .zip(prev)
                    .map(|(n, o)| {
                        if n.as_deref().is_some_and(|s| !s.is_empty()) {
                            n.clone()
                        } else {
                            o.clone()
                        }
                    })
                    .collect()
            })
            .collect();
        out.insert(key.clone(), filled);
    }
    out
}

/// One row per period, one exact decimal column per series, at the widest
/// scale that series uses.
pub fn series_to_table(key: &str, set: &SeriesSet, meta: BTreeMap<String, String>) -> RefTable {
    let keys: Vec<String> = set
        .values()
        .flat_map(|s| s.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut t = RefTable {
        names: vec![key.to_string()],
        columns: vec![RefColumn::Text(keys.iter().cloned().map(Some).collect())],
        meta,
    };
    for (name, series) in set {
        let scale = series.values().map(|v| v.scale()).max().unwrap_or(0);
        let values = keys
            .iter()
            .map(|k| series.get(k).and_then(|v| v.raw_scaled(scale)))
            .collect();
        t.names.push(name.clone());
        t.columns.push(RefColumn::Decimal { scale, values });
    }
    t
}

pub fn table_to_series(t: &RefTable, key: &str) -> anyhow::Result<SeriesSet> {
    let keys = t
        .text(key)
        .ok_or_else(|| anyhow::anyhow!("no {key} column"))?;
    let mut set = SeriesSet::new();
    for (name, col) in t.names.iter().zip(&t.columns) {
        let RefColumn::Decimal { scale, values } = col else {
            continue;
        };
        let series: Series = keys
            .iter()
            .zip(values)
            .filter_map(|(k, v)| Some((k.clone()?, Exact::from_raw((*v)?, *scale))))
            .collect();
        set.insert(name.clone(), series);
    }
    Ok(set)
}

/// Rows in key order; each row carries its own key column.
pub fn groups_to_table(
    names: &[&str],
    kinds: &[Kind],
    groups: &Groups,
    meta: BTreeMap<String, String>,
) -> RefTable {
    let rows: Vec<&Row> = groups.values().flatten().collect();
    let columns = kinds
        .iter()
        .enumerate()
        .map(|(c, kind)| match kind {
            Kind::Text => {
                RefColumn::Text(rows.iter().map(|r| r.get(c).cloned().flatten()).collect())
            }
            Kind::Int => RefColumn::Int(
                rows.iter()
                    .map(|r| r.get(c).cloned().flatten().and_then(|s| s.parse().ok()))
                    .collect(),
            ),
        })
        .collect();
    RefTable {
        names: names.iter().map(|s| s.to_string()).collect(),
        columns,
        meta,
    }
}

pub fn table_to_groups(t: &RefTable, key_of: impl Fn(&Row) -> String) -> Groups {
    let mut groups = Groups::new();
    for r in 0..t.rows() {
        let row: Row = t
            .columns
            .iter()
            .map(|c| match c {
                RefColumn::Text(v) => v[r].clone(),
                RefColumn::Int(v) => v[r].map(|i| i.to_string()),
                RefColumn::Decimal { scale, values } => {
                    values[r].map(|raw| Exact::from_raw(raw, *scale).text())
                }
            })
            .collect();
        groups.entry(key_of(&row)).or_default().push(row);
    }
    groups
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
