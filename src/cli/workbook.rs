//! `octa --to-workbook OUT.xlsx FILE...` - write several inputs into one
//! Excel workbook, one worksheet per input.
//!
//! The GUI's File > Export workbook does the same for open tabs; both call
//! `excel_reader::write_workbook`, so the sheet-naming rules cannot differ.
//! Sheet names come from the file stems and are corrected by
//! `sanitize_sheet_name` (31 characters, no forbidden punctuation, duplicates
//! numbered), which is what makes two inputs called `2024/q1.csv` and
//! `2024-q1.csv` land as distinct sheets rather than failing the write.

use std::path::PathBuf;

use anyhow::{Result, bail};

use octa::data::DataTable;
use octa::formats::excel_reader::write_workbook;

pub fn run(out: PathBuf, inputs: Vec<PathBuf>) -> Result<()> {
    if inputs.len() < 2 {
        bail!(
            "--to-workbook needs at least two input files (one sheet each); \
             for a single file use --convert"
        );
    }
    if out.extension().and_then(|e| e.to_str()).unwrap_or_default() != "xlsx" {
        bail!("--to-workbook writes .xlsx; got {}", out.display());
    }

    // Read everything before writing anything: a workbook half-written
    // because input 5 of 6 was unreadable is worse than no workbook.
    let mut tables: Vec<DataTable> = Vec::with_capacity(inputs.len());
    for path in &inputs {
        tables.push(super::read_table(path)?);
    }

    let names: Vec<String> = inputs
        .iter()
        .map(|p| {
            p.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sheet".to_string())
        })
        .collect();

    let sheets: Vec<(
        String,
        &DataTable,
        Option<&octa::formats::write_options::TableStyle>,
    )> = names
        .iter()
        .zip(tables.iter())
        .map(|(n, t)| (n.clone(), t, None))
        .collect();

    write_workbook(&out, &sheets)?;

    // Headerless `input\tsheet\trows`, matching --partition-by's listing: a
    // plain record of what was written, not a data table.
    let mut taken: Vec<String> = Vec::new();
    for ((path, name), table) in inputs.iter().zip(names.iter()).zip(tables.iter()) {
        let sheet = octa::formats::xlsx_style::sanitize_sheet_name(name, &mut taken);
        println!("{}\t{}\t{}", path.display(), sheet, table.row_count());
    }
    Ok(())
}
