//! Find columns (or small combinations of columns) whose values are
//! unique across a `DataTable`. Useful for primary-key reconnaissance
//! on undocumented databases or freshly-imported CSVs.
//!
//! Pure function so the MCP tool, CLI handler, and tests share one
//! implementation. Keyed by `duplicates::cell_key` joined with the ASCII
//! unit separator, the scheme `duplicates::find_duplicate_rows` uses, so a
//! "unique columns" answer here is consistent with a "no duplicates" answer
//! from the dedup tool.

use std::collections::HashSet;

use crate::data::DataTable;

/// Hard upper bound on `max_combo_size`. Above this the combo count
/// explodes (C(50, 4) ≈ 230k). The MCP / CLI callers clamp before
/// calling, but the library also clamps defensively.
pub const MAX_COMBO_SIZE: usize = 3;

/// Result of running [`find_unique_columns`].
#[derive(Debug, Clone)]
pub struct UniqueAnalysis {
    /// Total rows in the table at scan time.
    pub total_rows: usize,
    /// Per-column results, one entry per column in input order.
    pub single: Vec<UniqueResult>,
    /// Combo results (pairs / triples / ...). Empty unless the caller
    /// requested `max_combo_size > 1`. Listed in lexicographic column
    /// order (sorted by the column-index tuple) for determinism.
    pub combos: Vec<ComboUniqueResult>,
}

/// One single-column analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniqueResult {
    pub column: String,
    pub distinct_count: usize,
    pub null_count: usize,
    /// `true` when `distinct_count == total_rows` AND `null_count == 0`.
    /// A column with one `NULL` and otherwise distinct values is NOT
    /// considered a primary-key candidate because most databases
    /// reject `NULL` in a PK.
    pub is_unique: bool,
}

/// One multi-column analysis (pair / triple / etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComboUniqueResult {
    pub columns: Vec<String>,
    pub distinct_count: usize,
    pub is_unique: bool,
}

/// Scan `table` and return uniqueness information.
///
/// `max_combo_size` is clamped to `[1, MAX_COMBO_SIZE]`. Combos are
/// only generated when `max_combo_size > 1`. To avoid pointless work,
/// combos only consider columns whose own single-column
/// `distinct_count` is in `(1, total_rows)` - a column that's already
/// unique on its own would trivially make any combo unique, and a
/// column with one distinct value contributes nothing.
pub fn find_unique_columns(table: &DataTable, max_combo_size: usize) -> UniqueAnalysis {
    let total_rows = table.row_count();
    let col_count = table.col_count();
    let combo_cap = max_combo_size.clamp(1, MAX_COMBO_SIZE);

    let single: Vec<UniqueResult> = (0..col_count)
        .map(|col| analyse_single(table, col, total_rows))
        .collect();

    let mut combos = Vec::new();
    if combo_cap > 1 && total_rows > 0 {
        // Useful columns for combos: columns with more than one
        // distinct value (else they don't contribute) and fewer
        // distinct values than total rows (else they're already
        // unique on their own).
        let useful: Vec<usize> = single
            .iter()
            .enumerate()
            .filter(|(_, s)| s.distinct_count > 1 && s.distinct_count < total_rows)
            .map(|(i, _)| i)
            .collect();
        for size in 2..=combo_cap {
            for combo in combinations(&useful, size) {
                let (distinct, is_unique) = combo_distinct(table, &combo, total_rows);
                combos.push(ComboUniqueResult {
                    columns: combo
                        .iter()
                        .map(|&c| table.columns[c].name.clone())
                        .collect(),
                    distinct_count: distinct,
                    is_unique,
                });
            }
        }
    }

    UniqueAnalysis {
        total_rows,
        single,
        combos,
    }
}

fn analyse_single(table: &DataTable, col: usize, total_rows: usize) -> UniqueResult {
    use crate::data::CellValue;
    let mut seen: HashSet<String> = HashSet::with_capacity(total_rows.min(1024));
    let mut nulls = 0usize;
    for row in 0..total_rows {
        match table.get(row, col) {
            Some(CellValue::Null) | None => nulls += 1,
            Some(v) => {
                seen.insert(super::duplicates::cell_key(
                    &table.columns[col].data_type,
                    v,
                ));
            }
        }
    }
    let distinct_count = seen.len() + if nulls > 0 { 1 } else { 0 };
    let is_unique = nulls == 0 && distinct_count == total_rows && total_rows > 0;
    UniqueResult {
        column: table.columns[col].name.clone(),
        distinct_count,
        null_count: nulls,
        is_unique,
    }
}

fn combo_distinct(table: &DataTable, combo: &[usize], total_rows: usize) -> (usize, bool) {
    let mut seen: HashSet<String> = HashSet::with_capacity(total_rows.min(1024));
    let mut key = String::new();
    for row in 0..total_rows {
        key.clear();
        for &col in combo {
            if let Some(v) = table.get(row, col) {
                key.push_str(&super::duplicates::cell_key(
                    &table.columns[col].data_type,
                    v,
                ));
            }
            key.push('\x1F');
        }
        seen.insert(key.clone());
    }
    let distinct = seen.len();
    (distinct, distinct == total_rows && total_rows > 0)
}

/// All k-combinations of `xs` as borrowed slices. Order is
/// deterministic (lexicographic on the index positions within `xs`).
fn combinations(xs: &[usize], k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if k == 0 || k > xs.len() {
        return out;
    }
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.iter().map(|&i| xs[i]).collect());
        // Lex-next combination.
        let mut i = k;
        while i > 0 {
            i -= 1;
            if idx[i] + 1 < xs.len() - (k - 1 - i) {
                idx[i] += 1;
                for j in i + 1..k {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                return out;
            }
        }
        // Done condition: outermost can't advance further.
        if idx[0] > xs.len() - k {
            return out;
        }
    }
}
