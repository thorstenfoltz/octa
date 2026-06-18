use crate::data::{CellValue, DataTable};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutlierMethod {
    Iqr,
    ZScore,
}

fn numeric(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
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
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    #[test]
    fn iqr_flags_extreme_value() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        }];
        t.rows = (1..=10).map(|i| vec![CellValue::Int(i)]).collect();
        t.rows.push(vec![CellValue::Int(1000)]);
        let flagged = detect_outliers(&t, &[0], OutlierMethod::Iqr, 1.5);
        assert!(flagged.contains(&(10, 0)));
        assert!(!flagged.contains(&(0, 0)));
    }

    #[test]
    fn zscore_flags_extreme_value() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Float64".into(),
        }];
        // Nine values near 0, one extreme outlier
        t.rows = (0..9).map(|_| vec![CellValue::Float(0.0)]).collect();
        t.rows.push(vec![CellValue::Float(100.0)]);
        let flagged = detect_outliers(&t, &[0], OutlierMethod::ZScore, 2.0);
        assert!(flagged.contains(&(9, 0)));
        assert!(!flagged.contains(&(0, 0)));
    }

    #[test]
    fn skips_column_with_fewer_than_four_values() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        }];
        t.rows = vec![
            vec![CellValue::Int(1)],
            vec![CellValue::Int(2)],
            vec![CellValue::Int(1000)],
        ];
        let flagged = detect_outliers(&t, &[0], OutlierMethod::Iqr, 1.5);
        assert!(flagged.is_empty());
    }

    #[test]
    fn no_outliers_in_uniform_column() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Int64".into(),
        }];
        t.rows = (0..10).map(|_| vec![CellValue::Int(5)]).collect();
        // All same value: IQR = 0, so bounds collapse; none should be flagged
        // (sd = 0 path for ZScore skips the column)
        let flagged = detect_outliers(&t, &[0], OutlierMethod::ZScore, 1.5);
        assert!(flagged.is_empty());
    }
}
