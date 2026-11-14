//! `octa --outliers <FILE> [--outlier-method iqr|zscore|neighbours]
//!     [--outlier-cols COLS] [--outlier-k K]
//!     [--order-by COL --outlier-window N --outlier-same-weekday]`
//!
//! Flag numeric outlier cells per column using IQR, z-score or each value's
//! neighbours in the order of a date column, and print a
//! synthetic three-column result table: `row` (Int64), `column` (Utf8),
//! `value` (Utf8). One row per flagged cell, sorted by (row, col).

use octa::data::outliers::{OutlierMethod, detect_outliers};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub method: Option<String>,
    pub cols: Option<String>,
    pub k: Option<f64>,
    /// `--order-by`, the neighbours method's row order.
    pub order: Option<String>,
    pub window: Option<usize>,
    pub same_weekday: bool,
}

pub fn run(path: std::path::PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let cols_str = o.cols;

    let order = o
        .order
        .as_ref()
        .map(|n| table.column_index(n))
        .transpose()
        .map_err(|e| anyhow::anyhow!("--order-by: {e:#}"))?;
    let method = OutlierMethod::parse(o.method.as_deref(), order, o.window, o.same_weekday)
        .map_err(|e| anyhow::anyhow!("--outliers: {e}"))?;
    let k = o.k.unwrap_or(method.default_k());

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
