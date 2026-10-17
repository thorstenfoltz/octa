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

use crate::data::fuzzy_duplicates::{NormalizeOpts, normalize};
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
    /// Cells that differ exactly but count as the same under the options.
    pub within_tolerance: usize,
}

/// How forgiving a comparison is. `Default` is exact, and every caller that
/// syncs data (`sync_sql`) or highlights a statement's changes relies on that.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompareOptions {
    /// Numbers within this absolute difference count as the same.
    pub number_abs: Option<f64>,
    /// Numbers within this percentage of the larger one count as the same.
    pub number_pct: Option<f64>,
    /// Dates and times within this many seconds count as the same.
    pub seconds: Option<f64>,
    /// Text compares without regard to upper/lower case (join keys too).
    pub ignore_case: bool,
    /// Text compares trimmed, runs of whitespace as one space (join keys too).
    pub ignore_spaces: bool,
    /// Columns left out of the comparison; they still appear in the output.
    pub ignore_columns: Vec<String>,
}

impl CompareOptions {
    pub fn is_exact(&self) -> bool {
        *self == Self::default()
    }

    /// Refuse what cannot be honoured. Set mode matches rows by fingerprint,
    /// and a fingerprint cannot be "roughly equal".
    pub fn check(&self, mode: CompareMode) -> Result<(), &'static str> {
        let negative = [self.number_abs, self.number_pct, self.seconds]
            .into_iter()
            .flatten()
            .any(|v| v.is_nan() || v < 0.0);
        if negative {
            return Err("tolerances cannot be negative");
        }
        if mode == CompareMode::Set && !self.is_exact() {
            return Err("tolerance and ignore options need mode ordered or join");
        }
        Ok(())
    }

    fn normalises_text(&self) -> bool {
        self.ignore_case || self.ignore_spaces
    }

    fn norm(&self, s: &str) -> String {
        normalize(
            s,
            &NormalizeOpts {
                lower: self.ignore_case,
                collapse_ws: self.ignore_spaces,
                strip_punct: false,
            },
        )
    }

    fn ignores(&self, column: &str) -> bool {
        self.ignore_columns.iter().any(|c| c == column)
    }
}

/// How two cells compare under a set of options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Equality {
    Same,
    /// Different text, but within a tolerance or equal once normalised.
    WithinTolerance,
    Different,
}

/// Compare two cells. Exactly equal display text is always `Same`; the
/// options can only turn a difference into `WithinTolerance`, never the other
/// way. A null cell is never within tolerance of anything.
pub fn cells_equal(
    a: Option<&CellValue>,
    b: Option<&CellValue>,
    opts: &CompareOptions,
) -> Equality {
    let text = |v: Option<&CellValue>| v.map(|v| v.to_string()).unwrap_or_default();
    let (sa, sb) = (text(a), text(b));
    if sa == sb {
        return Equality::Same;
    }
    if opts.is_exact() {
        return Equality::Different;
    }
    let (Some(a), Some(b)) = (a, b) else {
        return Equality::Different;
    };
    if matches!(a, CellValue::Null) || matches!(b, CellValue::Null) {
        return Equality::Different;
    }
    if opts.normalises_text() && opts.norm(&sa) == opts.norm(&sb) {
        return Equality::WithinTolerance;
    }
    if (opts.number_abs.is_some() || opts.number_pct.is_some())
        && let Some((diff, larger)) = number_gap(a, b)
    {
        let abs_ok = opts.number_abs.is_some_and(|t| diff <= t);
        let pct_ok = opts.number_pct.is_some_and(|p| diff <= p / 100.0 * larger);
        // An infinite gap would pass any percent of an infinite value.
        return if diff.is_finite() && (abs_ok || pct_ok) {
            Equality::WithinTolerance
        } else {
            Equality::Different
        };
    }
    if let Some(secs) = opts.seconds
        && let (Some((x, _)), Some((y, _))) = (
            crate::data::time_calc::cell_to_datetime(a),
            crate::data::time_calc::cell_to_datetime(b),
        )
    {
        let apart = (x - y).num_milliseconds().unsigned_abs() as f64 / 1000.0;
        return if apart <= secs {
            Equality::WithinTolerance
        } else {
            Equality::Different
        };
    }
    Equality::Different
}

/// How far apart two numeric cells are, and the larger magnitude. Integers
/// subtract exactly: through `f64`, ids above 2^53 that differ by one would
/// come out equal.
fn number_gap(a: &CellValue, b: &CellValue) -> Option<(f64, f64)> {
    if let (Some(x), Some(y)) = (as_int(a), as_int(b)) {
        let larger = x.unsigned_abs().max(y.unsigned_abs()) as f64;
        return Some((
            (i128::from(x) - i128::from(y)).unsigned_abs() as f64,
            larger,
        ));
    }
    let (x, y) = (as_number(a)?, as_number(b)?);
    Some(((x - y).abs(), x.abs().max(y.abs())))
}

/// A cell as an integer: typed, or text that reads as a plain one.
fn as_int(v: &CellValue) -> Option<i64> {
    match v {
        CellValue::Int(i) => Some(*i),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A cell as a number: typed numbers, and text that reads as a plain one.
fn as_number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
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
/// back to B when A is the shorter side). Columns in `opts.ignore_columns`
/// are skipped.
pub fn compare_ordered(a: &DataTable, b: &DataTable, opts: &CompareOptions) -> CompareResult {
    let shared_cols = a.col_count().min(b.col_count());
    let shared_rows = a.row_count().min(b.row_count());
    let names: Vec<String> = (0..shared_cols)
        .map(|c| {
            a.columns
                .get(c)
                .or_else(|| b.columns.get(c))
                .map(|ci| ci.name.clone())
                .unwrap_or_else(|| format!("c{c}"))
        })
        .collect();
    let compared: Vec<usize> = (0..shared_cols)
        .filter(|&c| !opts.ignores(&names[c]))
        .collect();

    let mut result = CompareResult::default();
    for r in 0..shared_rows {
        let mut changed_columns = Vec::new();
        for &c in &compared {
            match cells_equal(a.get(r, c), b.get(r, c), opts) {
                Equality::Same => {}
                Equality::WithinTolerance => result.within_tolerance += 1,
                Equality::Different => changed_columns.push(names[c].clone()),
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

/// Build the join key for `row` from the given key column indices. Case and
/// spaces are normalised when the options say so; tolerances never apply to
/// keys, so rows pair on exact (normalised) values.
fn join_key(table: &DataTable, row: usize, key_cols: &[usize], opts: &CompareOptions) -> String {
    let mut key = String::new();
    for &c in key_cols {
        let s = cell_str(table, row, c);
        if opts.normalises_text() {
            key.push_str(&opts.norm(&s));
        } else {
            key.push_str(&s);
        }
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
    opts: &CompareOptions,
) -> anyhow::Result<CompareResult> {
    if key_cols.is_empty() {
        anyhow::bail!("join comparison needs at least one key column (--diff-on)");
    }
    if let Some(k) = key_cols.iter().find(|k| opts.ignores(k)) {
        anyhow::bail!("key column `{k}` cannot also be left out of the comparison");
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
        .filter(|(_, ci)| !opts.ignores(&ci.name))
        .filter_map(|(ai, ci)| col_index(b, &ci.name).map(|bi| (ai, bi, ci.name.clone())))
        .collect();

    // key -> queue of row indices, preserving input order.
    let mut b_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for r in 0..b.row_count() {
        b_groups
            .entry(join_key(b, r, &b_keys, opts))
            .or_default()
            .push(r);
    }
    // Track which b rows got matched so leftovers become only_in_b.
    let mut b_matched = vec![false; b.row_count()];

    let mut result = CompareResult::default();
    for ra in 0..a.row_count() {
        let key = join_key(a, ra, &a_keys, opts);
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
                    match cells_equal(a.get(ra, *ai), b.get(rb, *bi), opts) {
                        Equality::Same => {}
                        Equality::WithinTolerance => result.within_tolerance += 1,
                        Equality::Different => changed_columns.push(name.clone()),
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
#[path = "compare_tests.rs"]
mod tests;
