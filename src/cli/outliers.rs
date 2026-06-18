//! `octa --outliers <FILE> [--outlier-method iqr|zscore] [--outlier-cols COLS]
//!                         [--outlier-k K]`
//!
//! Flag numeric outlier cells per column using IQR or z-score and print a
//! synthetic three-column result table: `row` (Int64), `column` (Utf8),
//! `value` (Utf8). One row per flagged cell, sorted by (row, col).

use octa::data::outliers::{OutlierMethod, detect_outliers};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

/// Default k for IQR (distance in multiples of the interquartile range).
const DEFAULT_K_IQR: f64 = 1.5;
/// Default k for z-score (standard deviations from the mean).
const DEFAULT_K_ZSCORE: f64 = 3.0;

pub fn run(
    path: std::path::PathBuf,
    method_str: Option<String>,
    cols_str: Option<String>,
    k_opt: Option<f64>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;

    // Map method string to enum (default: IQR).
    let method = match method_str.as_deref() {
        None | Some("iqr") => OutlierMethod::Iqr,
        Some("zscore") => OutlierMethod::ZScore,
        Some(other) => anyhow::bail!("--outlier-method must be `iqr` or `zscore`, got `{other}`"),
    };

    // Pick default k based on method.
    let k = k_opt.unwrap_or(match method {
        OutlierMethod::Iqr => DEFAULT_K_IQR,
        OutlierMethod::ZScore => DEFAULT_K_ZSCORE,
    });

    // Resolve column names to indices (default: all columns).
    let col_indices: Vec<usize> = match cols_str {
        None => (0..table.col_count()).collect(),
        Some(ref s) => {
            let mut idxs = Vec::new();
            for name in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let idx = table
                    .columns
                    .iter()
                    .position(|c| c.name == name)
                    .ok_or_else(|| anyhow::anyhow!("--outlier-cols: no column named `{name}`"))?;
                idxs.push(idx);
            }
            idxs
        }
    };

    let flagged = detect_outliers(&table, &col_indices, method, k);

    // Collect into a sorted Vec<(row, col)> for deterministic output.
    let mut pairs: Vec<(usize, usize)> = flagged.into_iter().collect();
    pairs.sort_unstable();

    // Build result table with columns: row (Int64), column (Utf8), value (Utf8).
    let mut out = DataTable::empty();
    out.columns = vec![
        ColumnInfo {
            name: "row".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "column".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "Utf8".into(),
        },
    ];
    out.rows = pairs
        .into_iter()
        .map(|(r, c)| {
            let col_name = table.columns[c].name.clone();
            let cell_str = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
            vec![
                CellValue::Int(r as i64),
                CellValue::String(col_name),
                CellValue::String(cell_str),
            ]
        })
        .collect();

    write_table(&out, format)
}
