use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutlierMethod {
    Iqr,
    ZScore,
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
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// Flag numeric outlier cells in the given columns. IQR: outside
/// [q1 - k*IQR, q3 + k*IQR]; ZScore: |z| > k. Columns with fewer than 4
/// numeric values are skipped. Returns `(row, col)` coordinates.
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
