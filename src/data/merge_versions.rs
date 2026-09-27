//! Merge versions: two or more edited copies of one table, merged per row
//! and per cell, optionally against the original they were all edited from.
//!
//! **With an original** Octa knows who changed what, the way git does: a cell
//! only one version changed takes that change, several versions making the
//! same change is fine, and only genuinely different changes ask for a
//! decision. A row deleted by some versions and left alone by the rest is
//! deleted; deleted by some and edited by others, it asks.
//!
//! **Without an original** there is nothing to tell "changed" from
//! "unchanged", so every cell where the versions disagree asks, and a row any
//! version has is kept (an added row and a deleted one look the same).
//!
//! Rows are matched by the key column(s), by name; with no key, row position
//! is the key. A repeated key pairs in input order (the 2nd `id=7` with the
//! 2nd `id=7`). Columns are unioned by name; with an original, a column it
//! had and any version dropped is dropped. Cell equality is
//! `CellValue::to_string()`, like every other compare in the crate.
//!
//! Versions are addressed by index into the `versions` slice. Pure: the GUI
//! dialog, the CLI `--merge` and the MCP `merge_tables` tool share it.

use std::collections::{HashMap, HashSet};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Where a merged row stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStatus {
    /// Every version agrees with the original (or, without one, each other).
    Unchanged,
    /// At least one version changed it, without contradicting another.
    Changed,
    /// New since the original; without an original, missing from some version.
    Added,
    /// Needs a decision.
    Conflict,
}

impl RowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Changed => "changed",
            Self::Added => "added",
            Self::Conflict => "conflict",
        }
    }
}

/// One value a conflicting cell could take, and which versions hold it.
#[derive(Debug, Clone, PartialEq)]
pub struct CellOption {
    pub value: CellValue,
    pub from: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConflictKind {
    /// Versions changed one cell to different values. `values[v]` is what
    /// version `v` holds (`None`: it lacks the row or the column); `options`
    /// are the distinct values, the choice indexes into them.
    Cell {
        col: usize,
        original: Option<CellValue>,
        values: Vec<Option<CellValue>>,
        options: Vec<CellOption>,
    },
    /// Some versions deleted the row, others edited it. Choice 0 deletes it,
    /// 1 keeps the edited row.
    DeleteVsEdit {
        deleted_by: Vec<usize>,
        edited_by: Vec<usize>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// Row index in [`MergeResult::table`].
    pub row: usize,
    pub kind: ConflictKind,
    pub choice: Option<usize>,
}

impl Conflict {
    /// Settle in favour of version `v`: its value, or its keep/delete. A
    /// version with no value for this cell leaves the conflict open.
    pub fn prefer(&mut self, v: usize) {
        match &self.kind {
            ConflictKind::Cell { options, .. } => {
                if let Some(i) = options.iter().position(|o| o.from.contains(&v)) {
                    self.choice = Some(i);
                }
            }
            ConflictKind::DeleteVsEdit { deleted_by, .. } => {
                self.choice = Some(if deleted_by.contains(&v) { 0 } else { 1 });
            }
        }
    }
}

/// The merge before conflicts are settled. Conflicting cells hold their
/// first option until a choice is made.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub table: DataTable,
    pub status: Vec<RowStatus>,
    pub conflicts: Vec<Conflict>,
    /// How many versions were merged (the original not counted).
    pub versions: usize,
}

impl MergeResult {
    pub fn unresolved(&self) -> usize {
        self.conflicts.iter().filter(|c| c.choice.is_none()).count()
    }

    /// Settle every open conflict in favour of version `v`.
    pub fn prefer_all(&mut self, v: usize) {
        for c in self.conflicts.iter_mut().filter(|c| c.choice.is_none()) {
            c.prefer(v);
        }
    }

    /// Apply the choices. Errors while any conflict is still open.
    pub fn finish(&self) -> anyhow::Result<DataTable> {
        Ok(self.finish_with_status()?.0)
    }

    /// [`Self::finish`] plus each surviving row's status.
    pub fn finish_with_status(&self) -> anyhow::Result<(DataTable, Vec<RowStatus>)> {
        let open = self.unresolved();
        if open > 0 {
            anyhow::bail!("{open} conflict(s) still need a decision");
        }
        let mut table = self.table.clone();
        let mut drop = vec![false; table.row_count()];
        for c in &self.conflicts {
            let choice = c.choice.expect("checked above");
            match &c.kind {
                ConflictKind::Cell { col, options, .. } => {
                    table.rows[c.row][*col] = options[choice].value.clone();
                }
                ConflictKind::DeleteVsEdit { .. } => drop[c.row] = choice == 0,
            }
        }
        let status = (0..table.row_count())
            .filter(|&i| !drop[i])
            .map(|i| self.status[i])
            .collect();
        let mut i = 0;
        table.rows.retain(|_| {
            i += 1;
            !drop[i - 1]
        });
        Ok((table, status))
    }
}

/// Merge `versions`, against `original` when there is one. Needs two
/// versions, or one plus an original. Errors if a key column is missing.
pub fn merge_versions(
    original: Option<&DataTable>,
    versions: &[&DataTable],
    keys: &[String],
) -> anyhow::Result<MergeResult> {
    if versions.len() + usize::from(original.is_some()) < 2 {
        anyhow::bail!("merging needs at least two versions");
    }
    let columns = merged_columns(original, versions);
    let base_rows = original
        .map(|t| keyed_rows(t, keys, "the original"))
        .transpose()?;
    let ver_rows: Vec<KeyedRows> = versions
        .iter()
        .enumerate()
        .map(|(i, t)| keyed_rows(t, keys, &format!("version {}", i + 1)))
        .collect::<anyhow::Result<_>>()?;

    // Output order: the original's rows, then new keys in version order.
    let mut order: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for kr in base_rows.iter().chain(ver_rows.iter()) {
        for k in &kr.order {
            if seen.insert(k.as_str()) {
                order.push(k.as_str());
            }
        }
    }

    let mut out = DataTable::empty();
    out.columns = columns.clone();
    let mut status = Vec::new();
    let mut conflicts = Vec::new();

    for key in order {
        let b = base_rows.as_ref().and_then(|kr| kr.get(key));
        let present: Vec<(usize, usize)> = ver_rows
            .iter()
            .enumerate()
            .filter_map(|(v, kr)| kr.get(key).map(|r| (v, r)))
            .collect();
        if present.is_empty() {
            continue; // every version deleted it
        }
        let missing: Vec<usize> = (0..versions.len())
            .filter(|v| !present.iter().any(|(p, _)| p == v))
            .collect();
        let row_idx = out.rows.len();

        let mut row = Vec::with_capacity(columns.len());
        let mut changed = false;
        let mut cell_conflicts = Vec::new();
        for (col, info) in columns.iter().enumerate() {
            let base_v = b.map(|r| cell(original.expect("row implies table"), r, &info.name));
            let values: Vec<Option<CellValue>> = (0..versions.len())
                .map(|v| {
                    present
                        .iter()
                        .find(|(p, _)| *p == v)
                        .and_then(|&(_, r)| cell(versions[v], r, &info.name))
                })
                .collect();
            match decide(base_v.as_ref(), &values) {
                Decision::Keep(v) => row.push(v),
                Decision::Take(v) => {
                    changed = true;
                    row.push(v);
                }
                Decision::Ask(options) => {
                    row.push(options[0].value.clone());
                    cell_conflicts.push(Conflict {
                        row: row_idx,
                        kind: ConflictKind::Cell {
                            col,
                            original: base_v.flatten(),
                            values,
                            options,
                        },
                        choice: None,
                    });
                }
            }
        }
        let edited = changed || !cell_conflicts.is_empty();

        // Deleted by some versions: stands if nobody edited, asks otherwise.
        if b.is_some() && !missing.is_empty() {
            if !edited {
                continue;
            }
            let edited_by: Vec<usize> = present.iter().map(|(v, _)| *v).collect();
            conflicts.push(Conflict {
                row: row_idx,
                kind: ConflictKind::DeleteVsEdit {
                    deleted_by: missing.clone(),
                    edited_by,
                },
                choice: None,
            });
        }
        let has_conflict = !cell_conflicts.is_empty() || (b.is_some() && !missing.is_empty());
        conflicts.extend(cell_conflicts);
        out.rows.push(row);
        status.push(if has_conflict {
            RowStatus::Conflict
        } else if (original.is_some() && b.is_none()) || (original.is_none() && !missing.is_empty())
        {
            RowStatus::Added
        } else if edited {
            RowStatus::Changed
        } else {
            RowStatus::Unchanged
        });
    }

    Ok(MergeResult {
        table: out,
        status,
        conflicts,
        versions: versions.len(),
    })
}

enum Decision {
    /// Nobody changed it.
    Keep(CellValue),
    /// One change (possibly made by several versions alike).
    Take(CellValue),
    Ask(Vec<CellOption>),
}

/// One cell. `base`: `None` when the merge has no original row for it,
/// `Some(None)` when the original row lacks the column. `values[v]`: `None`
/// when version `v` has no say (no row, or no such column).
fn decide(base: Option<&Option<CellValue>>, values: &[Option<CellValue>]) -> Decision {
    let text = |v: &Option<CellValue>| v.as_ref().map(|c| c.to_string());
    // Distinct values among the versions that have one, first-seen order.
    let mut options: Vec<CellOption> = Vec::new();
    for (v, val) in values.iter().enumerate() {
        let Some(val) = val else { continue };
        match options
            .iter_mut()
            .find(|o| o.value.to_string() == val.to_string())
        {
            Some(o) => o.from.push(v),
            None => options.push(CellOption {
                value: val.clone(),
                from: vec![v],
            }),
        }
    }
    match base {
        Some(bv) => {
            let bt = text(bv);
            let changes: HashSet<String> = options
                .iter()
                .map(|o| o.value.to_string())
                .filter(|t| Some(t) != bt.as_ref())
                .collect();
            match changes.len() {
                0 => Decision::Keep(bv.clone().unwrap_or(CellValue::Null)),
                1 => {
                    let t = changes.into_iter().next().expect("one");
                    let o = options.into_iter().find(|o| o.value.to_string() == t);
                    Decision::Take(o.expect("from options").value)
                }
                _ => Decision::Ask(options),
            }
        }
        None => match options.len() {
            0 => Decision::Keep(CellValue::Null),
            1 => Decision::Keep(options.remove(0).value),
            _ => Decision::Ask(options),
        },
    }
}

/// Union of the column lists by name, original first. With an original, a
/// column it had and any version dropped is left out (deletion wins).
fn merged_columns(original: Option<&DataTable>, versions: &[&DataTable]) -> Vec<ColumnInfo> {
    let has = |t: &DataTable, name: &str| t.columns.iter().any(|c| c.name == name);
    let mut out: Vec<ColumnInfo> = Vec::new();
    for t in original.into_iter().chain(versions.iter().copied()) {
        for c in &t.columns {
            let dropped = original.is_some_and(|o| has(o, &c.name))
                && !versions.iter().all(|v| has(v, &c.name));
            if !dropped && !out.iter().any(|o| o.name == c.name) {
                out.push(c.clone());
            }
        }
    }
    out
}

/// The column most likely to identify a row: the best
/// [`join_keys::suggest_keys`](crate::data::join_keys::suggest_keys) pairing
/// of a column with itself across the first two tables, present in all.
pub fn suggest_key(tables: &[&DataTable]) -> Option<String> {
    use crate::data::join_keys::{DEFAULT_SAMPLE_ROWS, suggest_keys};
    let (a, b) = (tables.first()?, tables.get(1)?);
    suggest_keys(&[a, b], DEFAULT_SAMPLE_ROWS)
        .into_iter()
        .filter(|k| k.left.0 == 0 && k.right.0 == 1)
        .map(|k| (&a.columns[k.left.1].name, &b.columns[k.right.1].name))
        .find(|(x, y)| {
            x == y
                && tables
                    .iter()
                    .all(|t| t.columns.iter().any(|c| &c.name == *x))
        })
        .map(|(x, _)| x.clone())
}

/// The open decisions as a table for the headless surfaces: `row` (1-based),
/// `column`, `original`, then one column per version with what it holds
/// (`(deleted)` for a row it removed).
pub fn conflict_table(result: &MergeResult) -> DataTable {
    let s = |v: String| CellValue::String(v);
    let shown = |v: &Option<CellValue>| v.as_ref().map(|v| v.to_string()).unwrap_or_default();
    let mut out = DataTable::empty();
    let mut names = vec!["row".to_string(), "column".into(), "original".into()];
    names.extend((1..=result.versions).map(|i| format!("version_{i}")));
    out.columns = names
        .into_iter()
        .map(|name| ColumnInfo {
            name,
            data_type: "Utf8".to_string(),
        })
        .collect();
    for c in result.conflicts.iter().filter(|c| c.choice.is_none()) {
        let mut row = vec![s((c.row + 1).to_string())];
        match &c.kind {
            ConflictKind::Cell {
                col,
                original,
                values,
                ..
            } => {
                row.push(s(result.table.columns[*col].name.clone()));
                row.push(s(shown(original)));
                row.extend(values.iter().map(|v| s(shown(v))));
            }
            ConflictKind::DeleteVsEdit {
                deleted_by,
                edited_by,
            } => {
                row.push(s(String::new()));
                row.push(s(String::new()));
                row.extend((0..result.versions).map(|v| {
                    s(if deleted_by.contains(&v) {
                        "(deleted)".into()
                    } else if edited_by.contains(&v) {
                        "(edited)".into()
                    } else {
                        String::new()
                    })
                }));
            }
        }
        out.rows.push(row);
    }
    out
}

/// A table's rows by key, remembering first-seen order.
struct KeyedRows {
    order: Vec<String>,
    index: HashMap<String, usize>,
}

impl KeyedRows {
    fn get(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }
}

fn keyed_rows(table: &DataTable, keys: &[String], label: &str) -> anyhow::Result<KeyedRows> {
    let key_cols: Vec<usize> = keys
        .iter()
        .map(|name| {
            table
                .columns
                .iter()
                .position(|c| &c.name == name)
                .ok_or_else(|| anyhow::anyhow!("key column `{name}` not found in {label}"))
        })
        .collect::<anyhow::Result<_>>()?;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut order = Vec::with_capacity(table.row_count());
    let mut index = HashMap::with_capacity(table.row_count());
    for r in 0..table.row_count() {
        let mut key = if key_cols.is_empty() {
            r.to_string()
        } else {
            let mut k = String::new();
            for &c in &key_cols {
                k.push_str(&table.get(r, c).map(|v| v.to_string()).unwrap_or_default());
                k.push('\x1F');
            }
            k
        };
        let nth = seen.entry(key.clone()).or_insert(0);
        key.push_str(&format!("\x1E{nth}"));
        *nth += 1;
        index.insert(key.clone(), r);
        order.push(key);
    }
    Ok(KeyedRows { order, index })
}

fn cell(table: &DataTable, row: usize, col_name: &str) -> Option<CellValue> {
    let c = table.columns.iter().position(|ci| ci.name == col_name)?;
    table.get(row, c).cloned()
}

#[cfg(test)]
#[path = "merge_versions_tests.rs"]
mod tests;
