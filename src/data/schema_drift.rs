//! Which files in a set disagree about their columns.
//!
//! [`analyse`] and [`report_table`] are pure: they take schemas that somebody
//! else has already read, so they are testable without touching a disk and
//! cheap enough to run over hundreds of files.
//!
//! [`collect_schemas`] is the one part that reads. It lives here rather than
//! in a surface module because the CLI action, the MCP tool and the GUI dialog
//! all need exactly this walk, and a second copy would drift from the first.
//!
//! Files are grouped by an exact schema fingerprint rather than compared one
//! against another, because the useful answer for 500 Parquet parts is
//! "497 look like this, 3 look like that", not a 500-column matrix.

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatRegistry;
use std::path::Path;

/// Depth limit shared with dataset mode: deep enough for Hive partitioning,
/// shallow enough that a mistyped path cannot walk a whole home directory.
const MAX_DEPTH: usize = 8;

/// One file's schema as a scan collects it: `(label, columns)`.
///
/// Shared by every surface so the CLI, the MCP tool and the GUI dialog cannot
/// disagree about the shape they hand the engine.
pub type FileSchema = (String, Vec<ColumnInfo>);

/// A file a scan could not read: `(label, reason)`.
pub type SkippedFile = (String, String);

/// How a scan compares schemas.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DriftOptions {
    /// Fold column-name case before comparing, so `Amount` and `amount` are
    /// one column. Off by default: differing case is a real difference that
    /// some downstream tools care about.
    pub ignore_case: bool,
    /// Walk subdirectories. Off by default. Honoured by the **callers** when
    /// collecting files; the engine never touches a disk. Carried here so the
    /// three surfaces spell the option the same way.
    pub recursive: bool,
}

/// One distinct schema and every file that has it.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaVariant {
    pub files: Vec<String>,
    pub columns: Vec<ColumnInfo>,
}

/// The result of a scan.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DriftReport {
    /// Distinct schemas, most files first.
    pub variants: Vec<SchemaVariant>,
    /// Columns whose type differs between variants or which are absent from
    /// some, in report order.
    pub drifting_columns: Vec<String>,
    /// Files the caller could not read.
    pub skipped: Vec<SkippedFile>,
    pub has_drift: bool,
    /// Whether [`analyse`] folded column-name case.
    ///
    /// Carried so [`report_table`] can group names exactly as the scan did.
    /// Without it a folded scan that still has two variants for some other
    /// reason would split one column across two rows, one per spelling, each
    /// reported missing from the other variant, which is the very difference
    /// the caller asked to ignore.
    pub ignore_case: bool,
}

fn key(name: &str, ignore_case: bool) -> String {
    if ignore_case {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

fn fingerprint(columns: &[ColumnInfo], ignore_case: bool) -> Vec<(String, String)> {
    columns
        .iter()
        .map(|c| (key(&c.name, ignore_case), c.data_type.clone()))
        .collect()
}

/// Read every readable file's schema under `dir`, collecting failures rather
/// than aborting: one bad file in a folder of 500 must not lose the other 499.
///
/// The registry is a parameter so a caller scanning repeatedly does not build
/// one per call.
pub fn collect_schemas(
    dir: &Path,
    recursive: bool,
    registry: &FormatRegistry,
) -> (Vec<FileSchema>, Vec<SkippedFile>) {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    walk(dir, recursive, 0, registry, &mut out, &mut skipped);
    (out, skipped)
}

fn walk(
    dir: &Path,
    recursive: bool,
    depth: usize,
    registry: &FormatRegistry,
    out: &mut Vec<FileSchema>,
    skipped: &mut Vec<SkippedFile>,
) {
    let Ok(entries) = crate::ui::directory_tree::read_sorted_dir(dir) else {
        skipped.push((
            dir.display().to_string(),
            "unreadable directory".to_string(),
        ));
        return;
    };
    for path in entries {
        if path.is_dir() {
            if recursive && depth + 1 < MAX_DEPTH {
                walk(&path, recursive, depth + 1, registry, out, skipped);
            }
            continue;
        }
        // Recursive scans can meet the same file name in two folders, so the
        // label carries the whole path: the file name alone would collide and
        // the report would name the wrong file.
        let label = if recursive {
            path.display().to_string()
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string())
        };
        match registry.reader_for_path(&path) {
            Some(reader) => match reader.read_schema(&path) {
                Ok(columns) => out.push((label, columns)),
                Err(e) => skipped.push((label, e.to_string())),
            },
            None => skipped.push((label, "no reader for this extension".to_string())),
        }
    }
}

/// Group files by schema and name the columns that differ.
pub fn analyse(files: &[FileSchema], opts: &DriftOptions) -> DriftReport {
    let mut variants: Vec<SchemaVariant> = Vec::new();

    for (file, columns) in files {
        let this = fingerprint(columns, opts.ignore_case);
        let existing = variants
            .iter_mut()
            .find(|v| fingerprint(&v.columns, opts.ignore_case) == this);

        match existing {
            Some(v) => v.files.push(file.clone()),
            None => variants.push(SchemaVariant {
                files: vec![file.clone()],
                // First spelling encountered wins, so the report names a
                // column the way a real file spells it.
                columns: columns.clone(),
            }),
        }
    }

    variants.sort_by_key(|v| std::cmp::Reverse(v.files.len()));

    let has_drift = variants.len() > 1;
    let drifting_columns = if has_drift {
        column_order(&variants, opts.ignore_case)
            .into_iter()
            .filter(|name| !consistent(&variants, name, opts.ignore_case))
            .collect()
    } else {
        Vec::new()
    };

    DriftReport {
        variants,
        drifting_columns,
        skipped: Vec::new(),
        has_drift,
        ignore_case: opts.ignore_case,
    }
}

/// Every column name across all variants, first-seen order.
fn column_order(variants: &[SchemaVariant], ignore_case: bool) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for v in variants {
        for c in &v.columns {
            if keys.insert(key(&c.name, ignore_case)) {
                seen.push(c.name.clone());
            }
        }
    }
    seen
}

/// The type this variant gives a column, or `None` when it lacks it.
fn type_in(variant: &SchemaVariant, name: &str, ignore_case: bool) -> Option<String> {
    let wanted = key(name, ignore_case);
    variant
        .columns
        .iter()
        .find(|c| key(&c.name, ignore_case) == wanted)
        .map(|c| c.data_type.clone())
}

fn consistent(variants: &[SchemaVariant], name: &str, ignore_case: bool) -> bool {
    let mut types = variants.iter().map(|v| type_in(v, name, ignore_case));
    let Some(first) = types.next() else {
        return true;
    };
    first.is_some() && types.all(|t| t == first)
}

/// Render a report as a table: `status`, `column`, then one column per variant.
///
/// Rows that need attention sort first, because a scan is read top down and
/// the drift is the entire point of running it.
pub fn report_table(report: &DriftReport) -> DataTable {
    let mut out = DataTable::empty();
    if report.variants.is_empty() {
        return out;
    }

    out.columns = vec![
        ColumnInfo {
            name: "status".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "column".into(),
            data_type: "Utf8".into(),
        },
    ];
    for (i, v) in report.variants.iter().enumerate() {
        let files = v.files.len();
        let plural = if files == 1 { "file" } else { "files" };
        out.columns.push(ColumnInfo {
            name: format!("variant_{} ({files} {plural})", i + 1),
            data_type: "Utf8".into(),
        });
    }

    // Group names the same way the scan grouped them, or a folded scan would
    // report one column twice.
    let fold = report.ignore_case;
    let names = column_order(&report.variants, fold);
    let mut rows: Vec<(bool, Vec<CellValue>)> = Vec::new();

    for name in names {
        let types: Vec<Option<String>> = report
            .variants
            .iter()
            .map(|v| type_in(v, &name, fold))
            .collect();
        let missing = types.iter().filter(|t| t.is_none()).count();
        let distinct: std::collections::HashSet<&String> = types.iter().flatten().collect();

        let status = if missing > 0 {
            format!("missing in {missing}")
        } else if distinct.len() > 1 {
            "type varies".to_string()
        } else {
            "consistent".to_string()
        };
        let needs_attention = status != "consistent";

        let mut row = vec![CellValue::String(status), CellValue::String(name.clone())];
        for t in types {
            row.push(match t {
                Some(ty) => CellValue::String(ty),
                None => CellValue::Null,
            });
        }
        rows.push((needs_attention, row));
    }

    // Stable: attention first, original column order preserved within each group.
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    out.rows = rows.into_iter().map(|(_, r)| r).collect();
    out
}

#[cfg(test)]
#[path = "schema_drift_tests.rs"]
mod tests;
