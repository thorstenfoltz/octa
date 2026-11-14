use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutlierMethod {
    Iqr,
    ZScore,
    /// Each value against its neighbours in the order of column `order` (a
    /// date, a timestamp or a number): `window` points on each side, or only
    /// those on the same weekday. A busy December no longer hides a bad day,
    /// and a quiet Sunday is not flagged for being a Sunday.
    Neighbours {
        order: usize,
        window: usize,
        same_weekday: bool,
    },
}

impl OutlierMethod {
    /// The default `k` and the CLI / MCP spelling.
    pub fn default_k(self) -> f64 {
        match self {
            OutlierMethod::Iqr => 1.5,
            OutlierMethod::ZScore | OutlierMethod::Neighbours { .. } => 3.0,
        }
    }

    /// A method by its CLI / MCP name. `neighbours` needs the order column
    /// (already an index); the other two take none of the extras.
    pub fn parse(
        name: Option<&str>,
        order: Option<usize>,
        window: Option<usize>,
        same_weekday: bool,
    ) -> Result<Self, String> {
        let method = match name.map(str::to_ascii_lowercase).as_deref() {
            None | Some("iqr") => OutlierMethod::Iqr,
            Some("zscore") => OutlierMethod::ZScore,
            Some("neighbours" | "neighbors") => OutlierMethod::Neighbours {
                order: order.ok_or("the neighbours method needs an order column")?,
                window: window.unwrap_or(DEFAULT_WINDOW),
                same_weekday,
            },
            Some(other) => {
                return Err(format!(
                    "unknown method `{other}` (use iqr, zscore or neighbours)"
                ));
            }
        };
        if !matches!(method, OutlierMethod::Neighbours { .. })
            && (order.is_some() || window.is_some() || same_weekday)
        {
            return Err(
                "an order column, a window and same-weekday belong to the neighbours method".into(),
            );
        }
        if window == Some(0) {
            return Err("the window must be 1 or more".into());
        }
        Ok(method)
    }

    pub fn id(self) -> &'static str {
        match self {
            OutlierMethod::Iqr => "iqr",
            OutlierMethod::ZScore => "zscore",
            OutlierMethod::Neighbours { .. } => "neighbours",
        }
    }
}

/// Points on each side a value is compared with, when nothing else is said.
pub const DEFAULT_WINDOW: usize = 7;
/// Fewer neighbours than this say nothing about what is usual.
const MIN_NEIGHBOURS: usize = 4;

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// The positions in `seq` (values in order) that stand out from the `window`
/// values on each side: further than `k` robust standard deviations
/// (1.4826 x the median absolute deviation, the Hampel rule) from their
/// median. Where more than half the neighbours are identical that deviation
/// is 0, so the mean absolute deviation stands in; in a wholly flat
/// neighbourhood any other value stands out.
fn unusual(seq: &[f64], window: usize, k: f64) -> Vec<usize> {
    let mut out = Vec::new();
    let mut near: Vec<f64> = Vec::with_capacity(window * 2);
    for (i, &v) in seq.iter().enumerate() {
        near.clear();
        near.extend_from_slice(&seq[i.saturating_sub(window)..i]);
        near.extend_from_slice(&seq[i + 1..(i + 1 + window).min(seq.len())]);
        if near.len() < MIN_NEIGHBOURS {
            continue;
        }
        near.sort_by(|a, b| a.total_cmp(b));
        let mid = median(&near);
        let mut dev: Vec<f64> = near.iter().map(|x| (x - mid).abs()).collect();
        dev.sort_by(|a, b| a.total_cmp(b));
        let mut spread = 1.4826 * median(&dev);
        if spread == 0.0 {
            spread = 1.2533 * dev.iter().sum::<f64>() / dev.len() as f64;
        }
        let far = (v - mid).abs();
        if (spread == 0.0 && far > 0.0) || (spread > 0.0 && far / spread > k) {
            out.push(i);
        }
    }
    out
}

/// The `Neighbours` method for one column.
fn neighbour_outliers(
    table: &DataTable,
    col: usize,
    (order, window, same_weekday): (usize, usize, bool),
    k: f64,
    out: &mut HashSet<(usize, usize)>,
) {
    // (order key in seconds, row, value); rows without either are left out.
    let mut points: Vec<(f64, usize, f64)> = (0..table.row_count())
        .filter_map(|r| {
            let at = table
                .get(r, order)
                .and_then(crate::data::calculated::seconds)?;
            Some((at, r, table.get(r, col).and_then(numeric)?))
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    // One sequence, or one per weekday (1970-01-01 was a Thursday; only that
    // the seven days fall apart matters here, not their names).
    let buckets = if same_weekday { 7 } else { 1 };
    for b in 0..buckets {
        let seq: Vec<&(f64, usize, f64)> = points
            .iter()
            .filter(|p| !same_weekday || (p.0 / 86_400.0).floor().rem_euclid(7.0) as usize == b)
            .collect();
        let values: Vec<f64> = seq.iter().map(|p| p.2).collect();
        for i in unusual(&values, window.max(1), k) {
            out.insert((seq[i].1, col));
        }
    }
}

/// A cell as a number, or `None` when it is not one.
///
/// `NaN` and the infinities count as "not a number here": they are what R's
/// `write.csv` and `pandas.to_csv(na_rep='NaN')` put in a column for a missing
/// value, and `"NaN".parse::<f64>()` happily returns one. Statistics treat
/// them as missing, the way `na.rm` does - which also keeps them out of the
/// sort below, where a `NaN` used to make `partial_cmp` return `None` and
/// panic the whole app on the UI thread.
fn numeric(v: &CellValue) -> Option<f64> {
    let n = match v {
        CellValue::Int(i) => *i as f64,
        CellValue::Float(f) => *f,
        CellValue::String(s) => s.trim().parse().ok()?,
        // An amount of time; a clock time is left out, it wraps at midnight.
        CellValue::Duration(ns) => *ns as f64,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// Flag numeric outlier cells in the given columns. IQR: outside
/// [q1 - k*IQR, q3 + k*IQR]; ZScore: |z| > k; Neighbours: more than k robust
/// deviations from the median of the points around it. Columns with fewer
/// than 4 numeric values are skipped, as is the order column itself.
/// Returns `(row, col)` coordinates.
pub fn detect_outliers(
    table: &DataTable,
    cols: &[usize],
    method: OutlierMethod,
    k: f64,
) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    for &col in cols {
        let vals: Vec<(usize, f64)> = (0..table.row_count())
            .filter_map(|r| table.get(r, col).and_then(numeric).map(|v| (r, v)))
            .collect();
        if vals.len() < 4 {
            continue;
        }
        match method {
            OutlierMethod::Neighbours {
                order,
                window,
                same_weekday,
            } => {
                if col != order && order < table.col_count() {
                    neighbour_outliers(table, col, (order, window, same_weekday), k, &mut out);
                }
            }
            OutlierMethod::Iqr => {
                let mut sorted: Vec<f64> = vals.iter().map(|(_, v)| *v).collect();
                sorted.sort_by(|a, b| a.total_cmp(b));
                let q = |p: f64| {
                    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
                    sorted[idx]
                };
                let (q1, q3) = (q(0.25), q(0.75));
                let iqr = q3 - q1;
                let (lo, hi) = (q1 - k * iqr, q3 + k * iqr);
                for (r, v) in &vals {
                    if *v < lo || *v > hi {
                        out.insert((*r, col));
                    }
                }
            }
            OutlierMethod::ZScore => {
                let n = vals.len() as f64;
                let mean = vals.iter().map(|(_, v)| v).sum::<f64>() / n;
                let var = vals.iter().map(|(_, v)| (v - mean).powi(2)).sum::<f64>() / n;
                let sd = var.sqrt();
                if sd == 0.0 {
                    continue;
                }
                for (r, v) in &vals {
                    if ((v - mean) / sd).abs() > k {
                        out.insert((*r, col));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "outliers_tests.rs"]
mod tests;
