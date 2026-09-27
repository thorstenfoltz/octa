//! Fold a folder of drifted files into ONE table, with provenance.
//!
//! The other half of this already exists. [`crate::data::harmonise`] takes a
//! folder of files whose schemas have drifted apart and writes one
//! harmonised copy of each into an output folder, with progress, cancel and
//! a per-file refusal when a cast would lose values. This module adds the
//! combine half and nothing else: the same inputs folded into a single
//! table, with a column saying which file each row came from.
//!
//! It therefore re-implements no scanning, no schema collection and no type
//! widening. The walk is [`crate::data::schema_drift::collect_schemas`] and
//! the reconciliation is [`crate::data::union::plan_union`], which is what
//! keeps a combined folder and a harmonised folder agreeing about what the
//! union schema is.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::schema_drift::{SkippedFile, collect_schemas};
use crate::data::union::{plan_union, union_tables};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatRegistry;

/// The default name for the provenance column.
pub const DEFAULT_SOURCE_COLUMN: &str = "source_file";

/// How to combine a folder.
#[derive(Debug, Clone)]
pub struct CombineOptions {
    /// Folder to read. Files are labelled relative to it, which is what the
    /// provenance column records.
    pub root: PathBuf,
    /// Walk subdirectories as well.
    pub recursive: bool,
    /// Fold column-name case when matching columns across files, so `Amount`
    /// and `amount` become one column rather than two.
    pub ignore_case: bool,
    /// Name for the provenance column. Suffixed if the data already has a
    /// column by that name.
    pub source_column: String,
}

impl Default for CombineOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            recursive: false,
            ignore_case: false,
            source_column: DEFAULT_SOURCE_COLUMN.to_string(),
        }
    }
}

/// What a combine produced.
#[derive(Debug, Clone)]
pub struct CombineReport {
    /// Every row of every readable file, under the union schema.
    pub table: DataTable,
    /// How many files were read into it.
    pub files_read: usize,
    /// Files that were not read, and why: `(label, reason)`. A folder with
    /// one corrupt file still combines the rest, because refusing the whole
    /// run over one bad part is the least useful possible answer.
    pub skipped: Vec<SkippedFile>,
}

/// Pick a provenance column name that does not collide with the data.
///
/// A file may genuinely have a column called `source_file`, typed by a
/// person who had the same idea. Overwriting it would destroy data, so the
/// new column is suffixed instead. Decided ONCE across every input, so all
/// the rows land in the same column rather than in `source_file_2` for the
/// files that happened to collide and `source_file` for the rest.
fn free_source_name(existing: &[Vec<ColumnInfo>], wanted: &str, ignore_case: bool) -> String {
    let fold = |s: &str| {
        if ignore_case {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let taken: std::collections::HashSet<String> = existing
        .iter()
        .flat_map(|cols| cols.iter().map(|c| fold(&c.name)))
        .collect();
    if !taken.contains(&fold(wanted)) {
        return wanted.to_string();
    }
    for n in 2.. {
        let candidate = format!("{wanted}_{n}");
        if !taken.contains(&fold(&candidate)) {
            return candidate;
        }
    }
    unreachable!("the loop returns for some suffix")
}

/// Append the provenance column to `table`, filled with `label`.
fn stamp_source(table: &mut DataTable, column: &str, label: &str) {
    table.columns.push(ColumnInfo {
        name: column.to_string(),
        data_type: "Utf8".to_string(),
    });
    for row in &mut table.rows {
        row.push(CellValue::String(label.to_string()));
    }
}

/// Read every readable file under `opts.root` and fold them into one table.
///
/// `progress(done, total)` is called before each file. `cancel` is checked
/// before each file, so a run over a slow folder can be stopped without
/// waiting for it to finish.
pub fn combine_folder(
    opts: &CombineOptions,
    progress: &dyn Fn(usize, usize),
    cancel: &AtomicBool,
) -> anyhow::Result<CombineReport> {
    let registry = FormatRegistry::new();
    let (schemas, mut skipped) = collect_schemas(&opts.root, opts.recursive, &registry);
    if schemas.is_empty() {
        anyhow::bail!("no readable files in {}", opts.root.display());
    }

    let total = schemas.len();
    let mut tables: Vec<DataTable> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    for (done, (label, _)) in schemas.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }
        progress(done, total);
        let path = opts.root.join(label);
        // `read_table_auto` rather than a reader by extension: it also
        // handles the `.gz` / `.zst` wrappers, so a folder of compressed
        // parts combines like any other.
        match crate::formats::read_table_auto(&path, None, u64::MAX) {
            Ok(t) => {
                tables.push(t);
                labels.push(label.clone());
            }
            Err(e) => skipped.push((label.clone(), format!("{e:#}"))),
        }
    }
    progress(total, total);

    if tables.is_empty() {
        anyhow::bail!("no file in {} could be read", opts.root.display());
    }

    let source_column = free_source_name(
        &tables.iter().map(|t| t.columns.clone()).collect::<Vec<_>>(),
        &opts.source_column,
        opts.ignore_case,
    );
    for (table, label) in tables.iter_mut().zip(&labels) {
        stamp_source(table, &source_column, label);
    }

    let schema_refs: Vec<&[ColumnInfo]> = tables.iter().map(|t| t.columns.as_slice()).collect();
    let plan = plan_union(&schema_refs, opts.ignore_case);
    let table_refs: Vec<&DataTable> = tables.iter().collect();
    let table = union_tables(&table_refs, &plan)?;

    Ok(CombineReport {
        files_read: tables.len(),
        table,
        skipped,
    })
}

#[cfg(test)]
#[path = "normalise_folder_tests.rs"]
mod tests;
