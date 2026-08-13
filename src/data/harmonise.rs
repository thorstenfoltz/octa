//! Rewrite a folder of drifting files to one common schema.
//!
//! The write half of [`schema_drift`](crate::data::schema_drift), which reports
//! that 497 parts look like this and 3 look like that but cannot do anything
//! about it.
//!
//! Shaped like [`batch_convert`](crate::data::batch_convert): **decide the
//! whole run before doing any of it**, then execute without letting one bad
//! file abort the rest. Two safety properties are deliberate and load-bearing:
//!
//! * Output goes to a **new directory**. The inputs are never touched, so a
//!   harmonisation that turns out wrong costs disk space rather than data.
//! * A file whose values will not survive a cast is **refused**, not written
//!   with nulls in place of the values that failed. Silently emptying cells
//!   during a "make these files consistent" operation is the worst possible
//!   outcome, because the result looks clean.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::data::schema_drift::FileSchema;
use crate::data::{ColumnInfo, DataTable};

/// How a harmonisation run is configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmoniseOptions {
    /// The scanned directory, used to turn a `FileSchema` label back into a
    /// readable path.
    ///
    /// Necessary because `collect_schemas` labels a file by its **name** for a
    /// flat scan and by its **full path** for a recursive one, a display choice
    /// that suits the drift report and leaves the flat label unusable on its
    /// own. `root.join(label)` covers both: joining an absolute path discards
    /// the base.
    pub root: PathBuf,
    /// Where harmonised copies are written. Never the input directory.
    pub out_dir: PathBuf,
    /// Fold column-name case when matching, so `Amount` and `amount` are one
    /// column and the difference becomes a rename rather than a drop plus add.
    pub ignore_case: bool,
    /// Overwrite an existing file in `out_dir`.
    pub overwrite: bool,
}

/// One change to make to a file's columns.
///
/// Applied in the order drop, rename, cast, add, then reorder to the target,
/// matching how `transform_columns` sequences its own operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnOp {
    /// Column is not in the target. Recorded in `FileAction::dropped` too.
    Drop { name: String },
    /// Same column, different spelling (only possible with `ignore_case`).
    Rename { from: String, to: String },
    /// Same column, different type.
    Cast { name: String, to_type: String },
    /// Target column this file does not have; filled with nulls.
    AddNull { name: String, data_type: String },
}

/// What will happen to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    /// Already has the target schema; it is copied unchanged.
    AlreadyMatches,
    /// Will be rewritten.
    Harmonise,
    /// Will not be written, with the reason.
    Refused(String),
}

/// The plan for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAction {
    pub input: PathBuf,
    pub output: PathBuf,
    pub ops: Vec<ColumnOp>,
    /// Columns not present in the target. Reported separately from `ops`
    /// because this is the only lossy part of the operation and it must be
    /// visible before the user commits to the run.
    pub dropped: Vec<String>,
    pub status: FileStatus,
}

/// The whole run, decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmonisePlan {
    pub actions: Vec<FileAction>,
    pub target: Vec<ColumnInfo>,
}

impl HarmonisePlan {
    /// Files that will be rewritten.
    pub fn to_change(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| a.status == FileStatus::Harmonise)
            .count()
    }
    /// Files already in the target shape.
    pub fn already_matching(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| a.status == FileStatus::AlreadyMatches)
            .count()
    }
    /// Files that will not be written.
    pub fn refused(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| matches!(a.status, FileStatus::Refused(_)))
            .count()
    }
    /// Every column that will be dropped, across all files, deduplicated.
    pub fn all_dropped(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .actions
            .iter()
            .flat_map(|a| a.dropped.iter().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

fn key(name: &str, ignore_case: bool) -> String {
    if ignore_case {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

/// Decide the whole run.
///
/// Works from schemas alone and touches no disk: whether a *cast* is actually
/// safe depends on the values, so that check belongs to `run_harmonise`, which
/// has the data in hand. This records the intent.
pub fn plan_harmonise(
    files: &[FileSchema],
    target: &[ColumnInfo],
    opts: &HarmoniseOptions,
) -> HarmonisePlan {
    // Output names first, so a collision between two inputs is known before
    // any file is judged on its own merits.
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    let outputs: Vec<PathBuf> = files
        .iter()
        .map(|(label, _)| {
            let p = Path::new(label);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".to_string());
            *name_counts.entry(name.clone()).or_insert(0) += 1;
            opts.out_dir.join(name)
        })
        .collect();

    let actions = files
        .iter()
        .zip(outputs)
        .map(|((label, columns), output)| {
            let input = opts.root.join(label);
            let file_name = output
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // An empty target would empty every file. That is never what
            // anyone meant, so refuse rather than obey.
            if target.is_empty() {
                return FileAction {
                    input,
                    output,
                    ops: Vec::new(),
                    dropped: Vec::new(),
                    status: FileStatus::Refused("the target schema has no columns".to_string()),
                };
            }

            // Two inputs from different folders sharing a file name would write
            // the same output. batch_convert disambiguates with `_2`, which is
            // right when converting formats and wrong here: silently renaming
            // one part of a dataset hides exactly the ambiguity a
            // harmonisation run must not paper over. Refuse both.
            if name_counts.get(&file_name).copied().unwrap_or(0) > 1 {
                return FileAction {
                    input,
                    output,
                    ops: Vec::new(),
                    dropped: Vec::new(),
                    status: FileStatus::Refused(format!(
                        "two inputs would collide on the output name {file_name}"
                    )),
                };
            }

            let mut ops = Vec::new();
            let mut dropped = Vec::new();

            let have: HashMap<String, &ColumnInfo> = columns
                .iter()
                .map(|c| (key(&c.name, opts.ignore_case), c))
                .collect();
            let wanted: HashMap<String, &ColumnInfo> = target
                .iter()
                .map(|c| (key(&c.name, opts.ignore_case), c))
                .collect();

            for c in columns {
                if !wanted.contains_key(&key(&c.name, opts.ignore_case)) {
                    dropped.push(c.name.clone());
                    ops.push(ColumnOp::Drop {
                        name: c.name.clone(),
                    });
                }
            }
            for want in target {
                let k = key(&want.name, opts.ignore_case);
                match have.get(&k) {
                    Some(mine) => {
                        if mine.name != want.name {
                            ops.push(ColumnOp::Rename {
                                from: mine.name.clone(),
                                to: want.name.clone(),
                            });
                        }
                        if mine.data_type != want.data_type {
                            ops.push(ColumnOp::Cast {
                                name: want.name.clone(),
                                to_type: want.data_type.clone(),
                            });
                        }
                    }
                    None => ops.push(ColumnOp::AddNull {
                        name: want.name.clone(),
                        data_type: want.data_type.clone(),
                    }),
                }
            }

            // Same columns, same order, same types: nothing to do. Column order
            // counts, since a differing order is exactly what a downstream
            // positional reader trips over.
            let same_order = columns.len() == target.len()
                && columns
                    .iter()
                    .zip(target)
                    .all(|(a, b)| a.name == b.name && a.data_type == b.data_type);
            let status = if ops.is_empty() && same_order {
                FileStatus::AlreadyMatches
            } else {
                FileStatus::Harmonise
            };

            FileAction {
                input,
                output,
                ops,
                dropped,
                status,
            }
        })
        .collect();

    HarmonisePlan {
        actions,
        target: target.to_vec(),
    }
}

/// Reorder and reshape `table` to `target`, given a planned action.
///
/// Separated from the IO so it is testable without a filesystem: the runner
/// reads, calls this, and writes.
pub fn apply_to_table(
    table: &DataTable,
    target: &[ColumnInfo],
    ignore_case: bool,
) -> Result<DataTable, String> {
    let mut out = DataTable::empty();
    out.columns = target.to_vec();
    out.format_name = table.format_name.clone();

    let index: HashMap<String, usize> = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| (key(&c.name, ignore_case), i))
        .collect();

    let rows = table.row_count();
    out.rows = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(target.len());
        for want in target {
            match index.get(&key(&want.name, ignore_case)) {
                Some(&c) => row.push(
                    table
                        .get(r, c)
                        .cloned()
                        .unwrap_or(crate::data::CellValue::Null),
                ),
                None => row.push(crate::data::CellValue::Null),
            }
        }
        out.rows.push(row);
    }
    Ok(out)
}

/// What a run actually did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmoniseReport {
    /// Per file: `(input, output, status, dropped, detail)`.
    pub items: Vec<HarmoniseItem>,
    pub written: usize,
    pub refused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmoniseItem {
    pub input: PathBuf,
    pub output: PathBuf,
    pub status: FileStatus,
    pub dropped: Vec<String>,
    pub rows: usize,
}

/// Execute a plan.
///
/// One bad file never aborts the run: it becomes `Refused` with a reason and
/// the rest continue, matching `run_batch`. `cancel` is checked before each
/// file and `progress(done, total)` fires after each.
pub fn run_harmonise(
    plan: &HarmonisePlan,
    opts: &HarmoniseOptions,
    progress: &dyn Fn(usize, usize),
    cancel: &std::sync::atomic::AtomicBool,
    write_opts: &crate::formats::write_options::WriteOptions,
) -> HarmoniseReport {
    use std::sync::atomic::Ordering;
    let registry = crate::formats::FormatRegistry::new();
    let total = plan.actions.len();
    let mut items = Vec::with_capacity(total);

    for (done, action) in plan.actions.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // Already refused at planning time (collision, empty target).
        if let FileStatus::Refused(_) = action.status {
            items.push(HarmoniseItem {
                input: action.input.clone(),
                output: action.output.clone(),
                status: action.status.clone(),
                dropped: action.dropped.clone(),
                rows: 0,
            });
            progress(done + 1, total);
            continue;
        }

        let (status, rows) = match harmonise_one(&registry, action, &plan.target, opts, write_opts)
        {
            Ok(n) => (action.status.clone(), n),
            Err(why) => (FileStatus::Refused(why), 0),
        };
        items.push(HarmoniseItem {
            input: action.input.clone(),
            output: action.output.clone(),
            status,
            dropped: action.dropped.clone(),
            rows,
        });
        progress(done + 1, total);
    }

    let refused = items
        .iter()
        .filter(|i| matches!(i.status, FileStatus::Refused(_)))
        .count();
    HarmoniseReport {
        written: items.len() - refused,
        refused,
        items,
    }
}

/// Read, reshape, write. Returns the row count, or the reason it was refused.
fn harmonise_one(
    registry: &crate::formats::FormatRegistry,
    action: &FileAction,
    target: &[ColumnInfo],
    opts: &HarmoniseOptions,
    write_opts: &crate::formats::write_options::WriteOptions,
) -> Result<usize, String> {
    if !opts.overwrite && action.output.exists() {
        return Err(format!("{} already exists", action.output.display()));
    }

    // read_table_auto rather than the registry directly, so .gz and .zst
    // inputs need no extra handling here.
    let mut table = crate::formats::read_table_auto(
        &action.input,
        None,
        crate::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )
    .map_err(|e| format!("could not read: {e}"))?;

    // THE safety check. A cast whose values will not survive is refused, never
    // written with nulls where the values used to be: a harmonisation that
    // silently empties cells produces a file that looks clean and is not.
    // `can_convert_column` is the same check the cast itself runs, so this can
    // never refuse something that would in fact have worked, or allow
    // something that would then fail halfway.
    for op in &action.ops {
        if let ColumnOp::Cast { name, to_type } = op {
            let Some(idx) = table
                .columns
                .iter()
                .position(|c| key(&c.name, opts.ignore_case) == key(name, opts.ignore_case))
            else {
                continue; // added-as-null columns have nothing to cast
            };
            if !table.can_convert_column(idx, to_type) {
                return Err(format!(
                    "column {name} cannot become {to_type} without losing values"
                ));
            }
            table.convert_column(idx, to_type);
        }
    }

    let out = apply_to_table(&table, target, opts.ignore_case)?;

    if let Some(parent) = action.output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let writer = registry
        .reader_for_path(&action.output)
        .ok_or_else(|| format!("no writer for {}", action.output.display()))?;
    writer
        .write_file_with_options(&action.output, &out, write_opts)
        .map_err(|e| format!("could not write: {e}"))?;
    Ok(out.row_count())
}

/// The report as a table, so every surface prints it the same way.
pub fn report_table(report: &HarmoniseReport) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        "input",
        "output",
        "status",
        "rows",
        "dropped_columns",
        "reason",
    ]
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
            let (status, reason) = match &i.status {
                FileStatus::AlreadyMatches => ("already_matches", String::new()),
                FileStatus::Harmonise => ("harmonised", String::new()),
                FileStatus::Refused(why) => ("refused", why.clone()),
            };
            vec![
                crate::data::CellValue::String(i.input.display().to_string()),
                crate::data::CellValue::String(i.output.display().to_string()),
                crate::data::CellValue::String(status.to_string()),
                crate::data::CellValue::Int(i.rows as i64),
                crate::data::CellValue::String(i.dropped.join(", ")),
                crate::data::CellValue::String(reason),
            ]
        })
        .collect();
    t
}

#[cfg(test)]
#[path = "harmonise_tests.rs"]
mod tests;
