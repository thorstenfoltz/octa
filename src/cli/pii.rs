//! `octa --detect-pii <FILE> [--pii-sample N]`
//!
//! Scan FILE for likely PII columns and print a synthetic result: `column`
//! (Utf8), `kind` (Utf8), `confidence` (Float64), `by_name` (Boolean),
//! `value_match` (Float64). One row per finding. `by_name`/`value_match` show
//! how the confidence was reached (header signal vs fraction of matching
//! values).

use octa::data::pii::scan_pii;
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

/// Default number of rows sampled per column for PII detection.
const DEFAULT_SAMPLE: usize = 500;

pub fn run(
    path: std::path::PathBuf,
    sample_rows: Option<usize>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let sample = sample_rows.unwrap_or(DEFAULT_SAMPLE);
    let findings = scan_pii(&table, sample);

    let mut out = DataTable::empty();
    out.columns = ["column", "kind", "confidence", "by_name", "value_match"]
        .iter()
        .zip(["Utf8", "Utf8", "Float64", "Boolean", "Float64"])
        .map(|(name, ty)| ColumnInfo {
            name: (*name).into(),
            data_type: ty.into(),
        })
        .collect();
    out.rows = findings
        .into_iter()
        .map(|f| {
            let col_name = table
                .columns
                .get(f.column)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            vec![
                CellValue::String(col_name),
                CellValue::String(f.kind.id().to_string()),
                CellValue::Float(f.confidence),
                CellValue::Bool(f.by_name),
                CellValue::Float(f.value_match),
            ]
        })
        .collect();

    write_table(&out, format)
}
