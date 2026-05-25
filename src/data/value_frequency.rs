//! Per-column value-frequency counts — `df.value_counts()` equivalent.
//!
//! [`compute_value_frequency`] is the pure entry point. It accepts a
//! `DataTable` + column index, walks every row, and returns an ordered
//! list of `(label, count)` rows plus a few aggregate fields. The
//! dialog in `app/dialogs/value_frequency.rs` consumes the result and
//! renders it; this module has no UI dependencies so it's
//! integration-testable.
//!
//! Numeric binning (`BinningMode::Sturges`) groups numeric cells into
//! roughly Sturges-many half-open buckets `[lo, hi)`. The last bucket
//! is closed on the right (`[lo, hi]`) so the maximum value lands
//! somewhere. Null cells never enter a bin — they're surfaced via the
//! separate `nulls` count.

use crate::data::{CellValue, DataTable, is_numeric_data_type};

/// What to do with numeric columns. `None` (the default) reports raw
/// values; `Sturges` groups numerics into `ceil(1 + log2(n))` ranges,
/// clamped to `[MIN_BUCKETS, MAX_BUCKETS]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinningMode {
    #[default]
    None,
    Sturges,
}

const MIN_BUCKETS: usize = 5;
const MAX_BUCKETS: usize = 30;

/// One row in the value-frequency result.
#[derive(Debug, Clone)]
pub struct ValueFrequencyRow {
    /// Display label. For non-binned data this is the cell's string
    /// representation; for binned data it's a range like `[1.00, 5.00)`.
    pub label: String,
    /// How many rows had this value (or fell in this bin).
    pub count: usize,
}

/// Result of [`compute_value_frequency`]. The `rows` are sorted by
/// `count` descending, ties broken alphabetically by label so the UI
/// shows a deterministic ordering. `nulls`, `total_non_null`, and
/// `unique_count` are computed against the whole column even when the
/// caller asks for a top-N slice, so the footer can report accurate
/// totals.
#[derive(Debug, Clone)]
pub struct ValueFrequency {
    pub column_name: String,
    pub rows: Vec<ValueFrequencyRow>,
    /// Number of null / missing cells in the column.
    pub nulls: usize,
    /// Number of non-null cells. `nulls + total_non_null` always equals
    /// the column's row count.
    pub total_non_null: usize,
    /// How many distinct values / bins exist in the column. May be
    /// larger than `rows.len()` when the caller passed `top_n: Some(N)`.
    pub unique_count: usize,
    /// Whether the result was binned (numeric ranges) or raw values.
    pub binned: bool,
}

/// Compute value-frequency for one column.
///
/// `top_n: None` returns every distinct value; `Some(n)` truncates to
/// the `n` most common (after sorting). `binning` is only consulted for
/// numeric columns; on string / date / boolean / binary columns it's
/// silently ignored (we always show raw values for those).
pub fn compute_value_frequency(
    table: &DataTable,
    col_idx: usize,
    top_n: Option<usize>,
    binning: BinningMode,
) -> Option<ValueFrequency> {
    let col = table.columns.get(col_idx)?;
    let row_count = table.row_count();
    let column_name = col.name.clone();
    let numeric = is_numeric_data_type(&col.data_type);
    let do_bin = numeric && binning == BinningMode::Sturges;

    let mut nulls = 0usize;
    let mut numeric_values: Vec<f64> = Vec::new();
    // For the raw-value path we collect (label, count) in a Vec via a
    // sorted index — avoids pulling in a HashMap dep and keeps the
    // output stable.
    let mut counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(row_count.min(1024));

    for row in 0..row_count {
        match table.get(row, col_idx) {
            None | Some(CellValue::Null) => {
                nulls += 1;
            }
            Some(CellValue::String(s)) if s.is_empty() => {
                nulls += 1;
            }
            Some(value) => {
                if do_bin {
                    let n = match value {
                        CellValue::Int(n) => Some(*n as f64),
                        CellValue::Float(f) => Some(*f),
                        _ => None,
                    };
                    if let Some(n) = n {
                        if n.is_finite() {
                            numeric_values.push(n);
                        } else {
                            // ±inf / NaN: treat as raw value so the user
                            // still sees the cell, but skip the bin pass.
                            *counts.entry(format_special(n)).or_insert(0) += 1;
                        }
                    } else {
                        // Non-numeric stored in a numeric column. Surface
                        // verbatim so the user can spot type drift.
                        let key = value.to_string();
                        *counts.entry(key).or_insert(0) += 1;
                    }
                } else {
                    let key = value.to_string();
                    *counts.entry(key).or_insert(0) += 1;
                }
            }
        }
    }

    let total_non_null = row_count.saturating_sub(nulls);

    let mut rows: Vec<ValueFrequencyRow> = if do_bin && !numeric_values.is_empty() {
        let bins = sturges_bins(&numeric_values);
        // Merge any non-numeric leftovers (NaN/inf or type-drift cells)
        // into the same Vec so they appear in the result alongside the
        // bins.
        let mut all = bins;
        for (label, count) in counts.drain() {
            all.push(ValueFrequencyRow { label, count });
        }
        all
    } else {
        counts
            .into_iter()
            .map(|(label, count)| ValueFrequencyRow { label, count })
            .collect()
    };

    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    let unique_count = rows.len();
    if let Some(n) = top_n {
        rows.truncate(n);
    }

    Some(ValueFrequency {
        column_name,
        rows,
        nulls,
        total_non_null,
        unique_count,
        binned: do_bin,
    })
}

fn format_special(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n == f64::INFINITY {
        "+Inf".to_string()
    } else if n == f64::NEG_INFINITY {
        "-Inf".to_string()
    } else {
        // Unreachable in practice — caller only sends non-finite values.
        n.to_string()
    }
}

/// Build Sturges bins for `values` (must be non-empty and finite).
/// Returns `count == 0` bins too; the caller decides whether to show
/// them. `bin_count = ceil(1 + log2(n))`, clamped to `[5, 30]`.
fn sturges_bins(values: &[f64]) -> Vec<ValueFrequencyRow> {
    debug_assert!(!values.is_empty());

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // All values equal — one bucket, no division by zero.
    if (max - min).abs() < f64::EPSILON {
        return vec![ValueFrequencyRow {
            label: format!("[{}]", format_bin_bound(min)),
            count: values.len(),
        }];
    }

    let n = values.len() as f64;
    let raw = (1.0 + n.log2()).ceil() as usize;
    let bin_count = raw.clamp(MIN_BUCKETS, MAX_BUCKETS);

    let span = max - min;
    let width = span / bin_count as f64;
    let mut bins: Vec<(f64, f64, usize)> = (0..bin_count)
        .map(|i| {
            let lo = min + width * i as f64;
            let hi = if i + 1 == bin_count {
                max
            } else {
                min + width * (i + 1) as f64
            };
            (lo, hi, 0usize)
        })
        .collect();

    for &v in values {
        // Find the bin. Last bin is closed on the right so max lands.
        let mut idx = (((v - min) / width).floor() as i64).max(0) as usize;
        if idx >= bin_count {
            idx = bin_count - 1;
        }
        bins[idx].2 += 1;
    }

    bins.into_iter()
        .enumerate()
        .filter(|(_, (_, _, c))| *c > 0)
        .map(|(i, (lo, hi, c))| {
            let closed_right = i + 1 == bin_count;
            ValueFrequencyRow {
                label: format!(
                    "[{}, {}{}",
                    format_bin_bound(lo),
                    format_bin_bound(hi),
                    if closed_right { "]" } else { ")" }
                ),
                count: c,
            }
        })
        .collect()
}

fn format_bin_bound(v: f64) -> String {
    // Integer-valued floats render without a fractional part — keeps
    // bins of integer columns readable.
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{:.0}", v)
    } else {
        // Two decimals for normal ranges; fall back to default Display
        // for huge / tiny magnitudes where the precision would either
        // be misleading or look silly.
        if v.abs() < 1e-3 || v.abs() >= 1e6 {
            format!("{}", v)
        } else {
            format!("{:.2}", v)
        }
    }
}
