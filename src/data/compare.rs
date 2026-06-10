//! Richer table comparison than [`crate::data::diff`].
//!
//! `diff::diff_rows` answers "which whole rows are unique to each side" (a
//! set-membership test keyed on every column). This module adds two modes that
//! surface *cell-level* changes:
//!
//! * [`compare_ordered`] - positional: row `i` of A is compared with row `i` of
//!   B, cell by cell over the shared columns. Matched-but-differing rows are
//!   reported as [`RowChange`]s; trailing rows on the longer side become
//!   `only_in_a` / `only_in_b`.
//! * [`compare_join`] - keyed: rows are matched by one or more *key columns*
//!   (by name). Keys present on one side only become `only_in_a` / `only_in_b`;
//!   matched keys whose non-key cells differ become [`RowChange`]s naming the
//!   differing columns.
//!
//! Cell equality uses `CellValue::to_string()` (same convention as
//! [`crate::data::diff`] and the Compare view), so a Parquet row and a CSV row
//! with identical displayed values compare equal.
//!
//! All functions are pure so the CLI (`--diff --diff-mode`) and the MCP
//! `diff_tables` tool share them.

use std::collections::HashMap;

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Which comparison strategy to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    /// Whole-row set membership (delegates to [`crate::data::diff::diff_rows`]).
    Set,
    /// Positional: row `i` vs row `i`.
    Ordered,
    /// Keyed join on one or more columns.
    Join,
}

impl CompareMode {
    /// Parse the CLI / MCP string form. Returns `None` for unknown values.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "set" => Some(Self::Set),
            "ordered" => Some(Self::Ordered),
            "join" => Some(Self::Join),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Ordered => "ordered",
            Self::Join => "join",
        }
    }
}

/// A pair of matched rows (one from each side) whose cells differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowChange {
    /// Row index in A.
    pub row_a: usize,
    /// Row index in B.
    pub row_b: usize,
    /// Names of the columns whose values differ between the two rows.
    pub changed_columns: Vec<String>,
}

/// Outcome of [`compare_ordered`] / [`compare_join`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompareResult {
    /// Row indices in A that have no counterpart in B.
    pub only_in_a: Vec<usize>,
    /// Row indices in B that have no counterpart in A.
    pub only_in_b: Vec<usize>,
    /// Matched rows whose cells differ.
    pub changed: Vec<RowChange>,
    /// Count of matched rows that are identical.
    pub unchanged: usize,
}

/// Format a cell as a comparable string (Null included, so a Null vs empty
/// string still compares distinctly when displayed forms differ).
fn cell_str(table: &DataTable, row: usize, col: usize) -> String {
    table
        .get(row, col)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// Case-sensitive lookup of a column index by name.
fn col_index(table: &DataTable, name: &str) -> Option<usize> {
    table.columns.iter().position(|c| c.name == name)
}

/// Positional comparison. Rows are compared by index over the shared column
/// count (`min(a.cols, b.cols)`); differing columns are named from A (falling
/// back to B when A is the shorter side).
pub fn compare_ordered(a: &DataTable, b: &DataTable) -> CompareResult {
    let shared_cols = a.col_count().min(b.col_count());
    let shared_rows = a.row_count().min(b.row_count());

    let mut result = CompareResult::default();
    for r in 0..shared_rows {
        let mut changed_columns = Vec::new();
        for c in 0..shared_cols {
            if cell_str(a, r, c) != cell_str(b, r, c) {
                let name = a
                    .columns
                    .get(c)
                    .or_else(|| b.columns.get(c))
                    .map(|ci| ci.name.clone())
                    .unwrap_or_else(|| format!("c{c}"));
                changed_columns.push(name);
            }
        }
        if changed_columns.is_empty() {
            result.unchanged += 1;
        } else {
            result.changed.push(RowChange {
                row_a: r,
                row_b: r,
                changed_columns,
            });
        }
    }
    // Trailing rows on the longer side have no counterpart.
    result.only_in_a = (shared_rows..a.row_count()).collect();
    result.only_in_b = (shared_rows..b.row_count()).collect();
    result
}

/// Build the join key for `row` from the given key column indices.
fn join_key(table: &DataTable, row: usize, key_cols: &[usize]) -> String {
    let mut key = String::new();
    for &c in key_cols {
        key.push_str(&cell_str(table, row, c));
        key.push('\x1F');
    }
    key
}

/// Keyed comparison. Rows are matched by the named key column(s); within a key
/// group rows are paired in input order. Non-key columns shared by *name* are
/// compared; differing pairs become [`RowChange`]s. Returns an error if a key
/// column is missing from either table.
pub fn compare_join(
    a: &DataTable,
    b: &DataTable,
    key_cols: &[String],
) -> anyhow::Result<CompareResult> {
    if key_cols.is_empty() {
        anyhow::bail!("join comparison needs at least one key column (--diff-on)");
    }
    let a_keys: Vec<usize> = key_cols
        .iter()
        .map(|name| {
            col_index(a, name)
                .ok_or_else(|| anyhow::anyhow!("key column `{name}` not found in file A"))
        })
        .collect::<anyhow::Result<_>>()?;
    let b_keys: Vec<usize> = key_cols
        .iter()
        .map(|name| {
            col_index(b, name)
                .ok_or_else(|| anyhow::anyhow!("key column `{name}` not found in file B"))
        })
        .collect::<anyhow::Result<_>>()?;

    // Non-key columns shared by name (compared for each matched pair). Paired
    // as (a_col, b_col, name) so column order/position may differ between sides.
    let key_set: std::collections::HashSet<&str> = key_cols.iter().map(|s| s.as_str()).collect();
    let compare_cols: Vec<(usize, usize, String)> = a
        .columns
        .iter()
        .enumerate()
        .filter(|(_, ci)| !key_set.contains(ci.name.as_str()))
        .filter_map(|(ai, ci)| col_index(b, &ci.name).map(|bi| (ai, bi, ci.name.clone())))
        .collect();

    // key -> queue of row indices, preserving input order.
    let mut b_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for r in 0..b.row_count() {
        b_groups.entry(join_key(b, r, &b_keys)).or_default().push(r);
    }
    // Track which b rows got matched so leftovers become only_in_b.
    let mut b_matched = vec![false; b.row_count()];

    let mut result = CompareResult::default();
    for ra in 0..a.row_count() {
        let key = join_key(a, ra, &a_keys);
        let matched_rb = b_groups.get_mut(&key).and_then(|q| {
            if q.is_empty() {
                None
            } else {
                Some(q.remove(0))
            }
        });
        match matched_rb {
            Some(rb) => {
                b_matched[rb] = true;
                let mut changed_columns = Vec::new();
                for (ai, bi, name) in &compare_cols {
                    if cell_str(a, ra, *ai) != cell_str(b, rb, *bi) {
                        changed_columns.push(name.clone());
                    }
                }
                if changed_columns.is_empty() {
                    result.unchanged += 1;
                } else {
                    result.changed.push(RowChange {
                        row_a: ra,
                        row_b: rb,
                        changed_columns,
                    });
                }
            }
            None => result.only_in_a.push(ra),
        }
    }
    result.only_in_b = (0..b.row_count()).filter(|&r| !b_matched[r]).collect();
    Ok(result)
}

/// Materialise `indices` from `table` into a new `DataTable` (columns cloned).
pub fn subset(table: &DataTable, indices: &[usize]) -> DataTable {
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = indices
        .iter()
        .map(|&r| {
            (0..table.col_count())
                .map(|c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    out
}

/// Flatten a [`CompareResult`] into a single annotated `DataTable` for the CLI.
///
/// Columns: `status`, `changed_columns`, then the canonical data columns
/// (A's, or B's when A has none). Rows:
/// * `only_in_a` -> A's cells, `changed_columns` empty.
/// * `only_in_b` -> B's cells, `changed_columns` empty.
/// * each [`RowChange`] -> two rows, `changed_a` (A's cells) and `changed_b`
///   (B's cells), both naming the differing columns - so the before/after sit
///   adjacent.
pub fn build_compare_table(a: &DataTable, b: &DataTable, result: &CompareResult) -> DataTable {
    let canonical = if a.col_count() > 0 { a } else { b };
    let ncols = canonical.col_count();

    let mut columns = Vec::with_capacity(ncols + 2);
    columns.push(ColumnInfo {
        name: "status".to_string(),
        data_type: "Utf8".to_string(),
    });
    columns.push(ColumnInfo {
        name: "changed_columns".to_string(),
        data_type: "Utf8".to_string(),
    });
    columns.extend(canonical.columns.iter().cloned());

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut push = |table: &DataTable, r: usize, status: &str, changed: &str| {
        let mut row = Vec::with_capacity(ncols + 2);
        row.push(CellValue::String(status.to_string()));
        row.push(CellValue::String(changed.to_string()));
        for c in 0..ncols {
            row.push(table.get(r, c).cloned().unwrap_or(CellValue::Null));
        }
        rows.push(row);
    };

    for &r in &result.only_in_a {
        push(a, r, "only_in_a", "");
    }
    for &r in &result.only_in_b {
        push(b, r, "only_in_b", "");
    }
    for ch in &result.changed {
        let cols = ch.changed_columns.join(", ");
        push(a, ch.row_a, "changed_a", &cols);
        push(b, ch.row_b, "changed_b", &cols);
    }

    let mut out = DataTable::empty();
    out.columns = columns;
    out.rows = rows;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(headers: &[&str], rows: &[&[&str]]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = headers
            .iter()
            .map(|h| ColumnInfo {
                name: (*h).to_string(),
                data_type: "Utf8".to_string(),
            })
            .collect();
        t.rows = rows
            .iter()
            .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
            .collect();
        t
    }

    #[test]
    fn ordered_reports_changed_cells_and_trailing_rows() {
        let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
        let b = table(&["id", "v"], &[&["1", "x"], &["2", "Y"]]);
        let r = compare_ordered(&a, &b);
        assert_eq!(r.unchanged, 1); // row 0 identical
        assert_eq!(r.changed.len(), 1);
        assert_eq!(r.changed[0].row_a, 1);
        assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
        assert_eq!(r.only_in_a, vec![2]); // trailing "3,z"
        assert!(r.only_in_b.is_empty());
    }

    #[test]
    fn join_matches_added_removed_changed() {
        let a = table(&["id", "v"], &[&["1", "x"], &["2", "y"], &["3", "z"]]);
        let b = table(&["id", "v"], &[&["2", "Y"], &["3", "z"], &["4", "w"]]);
        let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
        assert_eq!(r.only_in_a, vec![0]); // id 1
        assert_eq!(r.only_in_b, vec![2]); // id 4
        assert_eq!(r.changed.len(), 1); // id 2: v changed
        assert_eq!(r.changed[0].changed_columns, vec!["v".to_string()]);
        assert_eq!(r.unchanged, 1); // id 3 identical
    }

    #[test]
    fn join_handles_columns_in_different_order() {
        let a = table(&["id", "v"], &[&["1", "x"]]);
        let b = table(&["v", "id"], &[&["x", "1"]]);
        let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
        assert_eq!(r.unchanged, 1);
        assert!(r.changed.is_empty());
    }

    #[test]
    fn join_duplicate_keys_pair_in_order() {
        let a = table(&["id", "v"], &[&["1", "a"], &["1", "b"]]);
        let b = table(&["id", "v"], &[&["1", "a"], &["1", "c"]]);
        let r = compare_join(&a, &b, &["id".to_string()]).unwrap();
        assert_eq!(r.unchanged, 1); // first 1,a pair
        assert_eq!(r.changed.len(), 1); // second 1,b vs 1,c
    }

    #[test]
    fn join_missing_key_column_errors() {
        let a = table(&["id"], &[&["1"]]);
        let b = table(&["other"], &[&["1"]]);
        assert!(compare_join(&a, &b, &["id".to_string()]).is_err());
    }

    #[test]
    fn build_table_emits_paired_changed_rows() {
        let a = table(&["id", "v"], &[&["1", "x"]]);
        let b = table(&["id", "v"], &[&["1", "X"]]);
        let r = compare_ordered(&a, &b);
        let out = build_compare_table(&a, &b, &r);
        assert_eq!(out.col_count(), 4); // status, changed_columns, id, v
        assert_eq!(out.row_count(), 2); // changed_a + changed_b
        assert_eq!(out.get(0, 0).unwrap().to_string(), "changed_a");
        assert_eq!(out.get(1, 0).unwrap().to_string(), "changed_b");
        assert_eq!(out.get(0, 1).unwrap().to_string(), "v");
    }
}
