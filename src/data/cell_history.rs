//! Where a cell's value came from: the versions of a file in which it
//! changed. Pure; `src/git/history.rs` loads the versions.
//!
//! The row is followed by a key, so a re-sorted file is not a history of
//! every row changing. Where the key is not unique (or missing) in a
//! version, that version is matched by position and `History::positional`
//! names it, for the dialog's banner.

use std::collections::HashSet;

use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    /// The file's path in this commit (renames change it); empty for the
    /// working copy and in tests.
    pub path: String,
}

impl CommitInfo {
    /// The not-yet-committed state of the file (or the open tab).
    pub fn is_working_copy(&self) -> bool {
        self.sha.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct Version {
    pub commit: CommitInfo,
    pub table: DataTable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellState {
    Value(String),
    RowAbsent,
    ColumnAbsent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub commit: CommitInfo,
    pub state: CellState,
    /// `None` for the oldest version loaded: what came before is not known.
    pub previous: Option<CellState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    /// Newest first; only versions where the cell changed, plus the oldest.
    pub entries: Vec<HistoryEntry>,
    /// Versions matched by position because the key did not work there.
    pub positional: Vec<CommitInfo>,
}

/// Which row to follow. `position` is the row's index in the newest
/// version, used where the key cannot be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowRef {
    Key {
        columns: Vec<String>,
        values: Vec<String>,
        position: usize,
    },
    Position(usize),
}

fn text(table: &DataTable, row: usize, col: usize) -> String {
    match table.get(row, col) {
        None | Some(CellValue::Null) => String::new(),
        Some(v) => v.to_string(),
    }
}

fn col_named(table: &DataTable, name: &str) -> Option<usize> {
    table.columns.iter().position(|c| c.name == name)
}

/// The key values of `row`, for building a [`RowRef::Key`].
pub fn key_values(table: &DataTable, columns: &[String], row: usize) -> Option<Vec<String>> {
    columns
        .iter()
        .map(|n| Some(text(table, row, col_named(table, n)?)))
        .collect()
}

/// First column whose non-empty values are all distinct: the fallback key
/// suggestion when there is no older version to compare with yet.
pub fn first_unique_column(table: &DataTable) -> Option<usize> {
    (0..table.col_count()).find(|&c| {
        let mut seen = HashSet::new();
        (0..table.row_count()).all(|r| {
            let t = text(table, r, c);
            !t.is_empty() && seen.insert(t)
        })
    })
}

enum Lookup {
    Found(usize),
    Missing,
    Unusable,
}

fn find_by_key(table: &DataTable, columns: &[String], values: &[String]) -> Lookup {
    let Some(idx) = columns
        .iter()
        .map(|n| col_named(table, n))
        .collect::<Option<Vec<_>>>()
    else {
        return Lookup::Unusable;
    };
    let mut seen = HashSet::new();
    let mut hit = None;
    for r in 0..table.row_count() {
        let k: Vec<String> = idx.iter().map(|&c| text(table, r, c)).collect();
        if k == values {
            hit = Some(r);
        }
        if !seen.insert(k) {
            return Lookup::Unusable;
        }
    }
    hit.map_or(Lookup::Missing, Lookup::Found)
}

/// The cell's state in one version, and whether position had to stand in
/// for the key.
fn state_in(table: &DataTable, row: &RowRef, column: &str) -> (CellState, bool) {
    let Some(col) = col_named(table, column) else {
        return (CellState::ColumnAbsent, false);
    };
    let by_position = |p: usize| (p < table.row_count()).then_some(p);
    let (found, positional) = match row {
        RowRef::Position(p) => (by_position(*p), false),
        RowRef::Key {
            columns,
            values,
            position,
        } => match find_by_key(table, columns, values) {
            Lookup::Found(r) => (Some(r), false),
            Lookup::Missing => (None, false),
            Lookup::Unusable => (by_position(*position), true),
        },
    };
    let state = found.map_or(CellState::RowAbsent, |r| {
        CellState::Value(text(table, r, col))
    });
    (state, positional)
}

/// Follow one cell through `versions` (newest first).
pub fn cell_history(versions: &[Version], row: &RowRef, column: &str) -> History {
    let mut positional = Vec::new();
    let states: Vec<CellState> = versions
        .iter()
        .map(|v| {
            let (state, fell_back) = state_in(&v.table, row, column);
            if fell_back {
                positional.push(v.commit.clone());
            }
            state
        })
        .collect();
    let entries = (0..versions.len())
        .filter_map(|i| {
            let previous = states.get(i + 1).cloned();
            (previous.as_ref() != Some(&states[i])).then(|| HistoryEntry {
                commit: versions[i].commit.clone(),
                state: states[i].clone(),
                previous,
            })
        })
        .collect();
    History {
        entries,
        positional,
    }
}

/// The row the headless surfaces name: key columns with their values
/// (followed through re-sorts), or a 1-based position.
pub fn resolve_row(
    versions: &[Version],
    keys: Vec<String>,
    values: Vec<String>,
    row_1based: Option<usize>,
) -> anyhow::Result<RowRef> {
    if !keys.is_empty() {
        anyhow::ensure!(
            keys.len() == values.len(),
            "the key names {} column(s) but {} value(s) were given",
            keys.len(),
            values.len()
        );
        let newest = versions
            .first()
            .ok_or_else(|| anyhow::anyhow!("no readable version of the file"))?;
        let position = (0..newest.table.row_count())
            .find(|&r| key_values(&newest.table, &keys, r).as_ref() == Some(&values))
            .unwrap_or(usize::MAX);
        return Ok(RowRef::Key {
            columns: keys,
            values,
            position,
        });
    }
    match row_1based {
        Some(n) if n >= 1 => Ok(RowRef::Position(n - 1)),
        _ => anyhow::bail!("name the row by a key and its value(s), or by its 1-based row number"),
    }
}

/// `change` column words; the GUI shows translated text, the headless
/// surfaces these stable ids.
pub fn change_id(e: &HistoryEntry) -> &'static str {
    match (&e.previous, &e.state) {
        (None, _) => "earliest",
        (Some(CellState::RowAbsent), CellState::Value(_)) => "row_added",
        (Some(_), CellState::RowAbsent) => "row_removed",
        (Some(_), CellState::ColumnAbsent) => "column_removed",
        (Some(CellState::ColumnAbsent), _) => "column_added",
        _ => "changed",
    }
}

/// The history as a table for the headless surfaces.
pub fn history_table(h: &History) -> DataTable {
    let s = |v: &str| CellValue::String(v.into());
    let mut out = DataTable::empty();
    out.columns = ["commit", "date", "author", "subject", "value", "change"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    out.rows = h
        .entries
        .iter()
        .map(|e| {
            let value = match &e.state {
                CellState::Value(v) => s(v),
                _ => CellValue::Null,
            };
            let sha = if e.commit.is_working_copy() {
                "(not committed)"
            } else {
                &e.commit.sha
            };
            vec![
                s(sha),
                s(&e.commit.date),
                s(&e.commit.author),
                s(&e.commit.subject),
                value,
                s(change_id(e)),
            ]
        })
        .collect();
    out
}

#[cfg(test)]
#[path = "cell_history_tests.rs"]
mod tests;
