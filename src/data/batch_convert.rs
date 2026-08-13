//! Batch conversion: N input files to one target format in one run.
//!
//! `plan_batch` is pure and decides everything that can fail before any work
//! starts: output names, collisions between two inputs wanting the same name,
//! existing files, and a target format nothing can write. `run_batch` then
//! executes the plan, recording each outcome and never aborting the run because
//! one file went wrong.
//!
//! **Ceiling:** a multi-table input (an Excel workbook, a database file)
//! converts its **first table only**. Handling every sheet needs an
//! output-naming scheme, which is a separate decision from this one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::{FormatRegistry, compression, read_table_auto};

/// Why an item will not be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The output already exists and overwriting was not asked for.
    ExistingFile,
    /// No writer for the target extension, or its format is read-only.
    ReadOnlyTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BatchStatus {
    Pending,
    Skipped(SkipReason),
    Done { rows: usize },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchItem {
    pub input: PathBuf,
    pub output: PathBuf,
    pub status: BatchStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchReport {
    pub items: Vec<BatchItem>,
    pub converted: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Decide the whole run before doing any of it.
///
/// Output names are `<out_dir>/<input stem>.<target_ext>`. Two inputs whose
/// stems collide get `_2`, `_3` suffixes in input order, so a batch can never
/// silently overwrite its own earlier output.
pub fn plan_batch(
    inputs: &[PathBuf],
    out_dir: &Path,
    target_ext: &str,
    overwrite: bool,
) -> Vec<BatchItem> {
    let ext = target_ext.trim_start_matches('.').to_ascii_lowercase();

    // One registry lookup for the whole batch: either the target is writable
    // or the entire run is pointless.
    //
    // The extension must be **claimed** by a reader, checked separately from
    // asking that reader whether it writes. `reader_for_path` deliberately
    // falls back to the Text reader for anything it does not recognise, which
    // is right for opening a file and wrong for picking a write target: it
    // would accept `--to zzz` and quietly emit a text dump named `.zzz`.
    let target_writable = {
        let registry = FormatRegistry::new();
        registry.all_extensions().contains(&ext)
            && registry
                .reader_for_path(Path::new(&format!("_check_.{ext}")))
                .map(|r| r.supports_write())
                .unwrap_or(false)
    };

    let mut used: HashMap<String, usize> = HashMap::new();
    inputs
        .iter()
        .map(|input| {
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".to_string());
            let count = used.entry(stem.clone()).or_insert(0);
            *count += 1;
            let name = if *count == 1 {
                format!("{stem}.{ext}")
            } else {
                format!("{stem}_{count}.{ext}")
            };
            let output = out_dir.join(name);

            let status = if !target_writable {
                BatchStatus::Skipped(SkipReason::ReadOnlyTarget)
            } else if !overwrite && output.exists() {
                BatchStatus::Skipped(SkipReason::ExistingFile)
            } else {
                BatchStatus::Pending
            };

            BatchItem {
                input: input.clone(),
                output,
                status,
            }
        })
        .collect()
}

/// Execute a plan. `progress(done, total)` fires after each converted item;
/// `cancel` is checked before each one.
pub fn run_batch(
    mut plan: Vec<BatchItem>,
    progress: &dyn Fn(usize, usize),
    cancel: &AtomicBool,
    opts: &crate::formats::write_options::WriteOptions,
) -> BatchReport {
    let total = plan.len();
    let registry = FormatRegistry::new();

    for (done, item) in plan.iter_mut().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if item.status != BatchStatus::Pending {
            continue;
        }
        item.status = match convert_one(&registry, &item.input, &item.output, opts) {
            Ok(rows) => BatchStatus::Done { rows },
            Err(e) => BatchStatus::Failed(e.to_string()),
        };
        progress(done + 1, total);
    }

    let converted = plan
        .iter()
        .filter(|i| matches!(i.status, BatchStatus::Done { .. }))
        .count();
    let failed = plan
        .iter()
        .filter(|i| matches!(i.status, BatchStatus::Failed(_)))
        .count();
    let skipped = plan
        .iter()
        .filter(|i| matches!(i.status, BatchStatus::Skipped(_)))
        .count();

    BatchReport {
        items: plan,
        converted,
        failed,
        skipped,
    }
}

fn convert_one(
    registry: &FormatRegistry,
    input: &Path,
    output: &Path,
    opts: &crate::formats::write_options::WriteOptions,
) -> anyhow::Result<usize> {
    // read_table_auto, not the registry directly, so .gz and .zst inputs work
    // with no extra code here.
    let table = read_table_auto(input, None, compression::DEFAULT_MAX_DECOMPRESSED_BYTES)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let writer = registry
        .reader_for_path(output)
        .ok_or_else(|| anyhow::anyhow!("no writer for {}", output.display()))?;
    writer.write_file_with_options(output, &table, opts)?;
    Ok(table.row_count())
}

/// The report as a table, so the GUI can open it in a tab and the CLI and MCP
/// can print it without a second formatting path.
pub fn report_table(report: &BatchReport) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = ["input", "output", "status", "rows", "error"]
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: if *n == "rows" { "Int64" } else { "Utf8" }.to_string(),
        })
        .collect();
    t.rows = report
        .items
        .iter()
        .map(|i| {
            let (status, rows, error) = match &i.status {
                BatchStatus::Pending => ("pending", CellValue::Null, String::new()),
                BatchStatus::Skipped(SkipReason::ExistingFile) => {
                    ("skipped", CellValue::Null, "output exists".to_string())
                }
                BatchStatus::Skipped(SkipReason::ReadOnlyTarget) => (
                    "skipped",
                    CellValue::Null,
                    "target format cannot be written".to_string(),
                ),
                BatchStatus::Done { rows } => ("done", CellValue::Int(*rows as i64), String::new()),
                BatchStatus::Failed(e) => ("failed", CellValue::Null, e.clone()),
            };
            vec![
                CellValue::String(i.input.display().to_string()),
                CellValue::String(i.output.display().to_string()),
                CellValue::String(status.to_string()),
                rows,
                CellValue::String(error),
            ]
        })
        .collect();
    t.structural_changes = true;
    t
}

#[cfg(test)]
#[path = "batch_convert_tests.rs"]
mod tests;
