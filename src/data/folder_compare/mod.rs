//! Compare N folders file by file.
//!
//! Pure: listing and hashing live in [`scan`]. Files are matched by their
//! path inside each folder. Equal size alone never makes two files "the
//! same": copies of equal size in two or more folders are hashed (BLAKE3
//! over the whole file), while a size difference already means "different"
//! without reading anything, and a file in one folder only is never read.
//!
//! The status words are stable English (`same`, `different`, `missing`):
//! the CLI and MCP tool emit them for scripts.

pub mod scan;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::data::{CellValue, ColumnInfo, DataTable};

pub const STATUS_SAME: &str = "same";
pub const STATUS_DIFFERENT: &str = "different";
pub const STATUS_MISSING: &str = "missing";

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    /// Path inside its folder, `/`-separated.
    pub rel_path: String,
    pub size: u64,
    pub modified: Option<DateTime<Utc>>,
    /// BLAKE3 of the contents, filled only where [`which_need_hash`] asked.
    pub hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderListing {
    /// What the user picked: a path or a URL.
    pub root: String,
    /// Short name for column headers, from [`labels_for`].
    pub label: String,
    pub files: Vec<FileEntry>,
    /// The listing stopped at the file limit.
    pub truncated: bool,
}

fn segments(root: &str) -> Vec<String> {
    let rest = root.split_once("://").map(|(_, r)| r).unwrap_or(root);
    rest.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Folder names for column headers: the last path segment, with parent
/// segments added until labels differ; identical roots get ` (2)`, ` (3)`.
pub fn labels_for(roots: &[String]) -> Vec<String> {
    let segs: Vec<Vec<String>> = roots.iter().map(|r| segments(r)).collect();
    let mut depth = vec![1usize; roots.len()];
    let label = |i: usize, d: usize| -> String {
        let s = &segs[i];
        if s.is_empty() {
            return "/".to_string();
        }
        s[s.len().saturating_sub(d)..].join("/")
    };
    loop {
        let labels: Vec<String> = (0..roots.len()).map(|i| label(i, depth[i])).collect();
        let mut grew = false;
        for i in 0..roots.len() {
            let clash = (0..roots.len()).any(|j| j != i && labels[j] == labels[i]);
            if clash && depth[i] < segs[i].len() {
                depth[i] += 1;
                grew = true;
            }
        }
        if !grew {
            let mut seen: std::collections::HashMap<String, usize> = Default::default();
            return labels
                .into_iter()
                .map(|l| {
                    let n = seen.entry(l.clone()).or_insert(0);
                    *n += 1;
                    if *n == 1 { l } else { format!("{l} ({n})") }
                })
                .collect();
        }
    }
}

/// `rel_path -> one slot per folder`.
fn by_path(listings: &[FolderListing]) -> BTreeMap<&str, Vec<Option<&FileEntry>>> {
    let mut map: BTreeMap<&str, Vec<Option<&FileEntry>>> = BTreeMap::new();
    for (i, l) in listings.iter().enumerate() {
        for f in &l.files {
            map.entry(f.rel_path.as_str())
                .or_insert_with(|| vec![None; listings.len()])[i] = Some(f);
        }
    }
    map
}

/// `(folder, rel_path)` of every file whose contents decide the answer: a
/// size shared by copies in two or more folders.
pub fn which_need_hash(listings: &[FolderListing]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (path, slots) in by_path(listings) {
        let mut by_size: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        for (i, slot) in slots.iter().enumerate() {
            if let Some(f) = slot {
                by_size.entry(f.size).or_default().push(i);
            }
        }
        for folders in by_size.values().filter(|v| v.len() >= 2) {
            out.extend(folders.iter().map(|&i| (i, path.to_string())));
        }
    }
    out.sort();
    out
}

fn hex(h: &[u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// One row per relative path, sorted: `path`, `status`, `in`, `missing
/// from`, then `size: L`, `modified: L`, `hash: L` per folder label `L`.
pub fn compare_table(listings: &[FolderListing]) -> DataTable {
    let col = |name: String, ty: &str| ColumnInfo {
        name,
        data_type: ty.to_string(),
    };
    let mut columns = vec![
        col("path".into(), "Utf8"),
        col("status".into(), "Utf8"),
        col("in".into(), "Utf8"),
        col("missing from".into(), "Utf8"),
    ];
    for l in listings {
        columns.push(col(format!("size: {}", l.label), "Int64"));
        columns.push(col(
            format!("modified: {}", l.label),
            "Timestamp(Microsecond, None)",
        ));
        columns.push(col(format!("hash: {}", l.label), "Utf8"));
    }
    let rows = by_path(listings)
        .into_iter()
        .map(|(path, slots)| {
            let present: Vec<&FileEntry> = slots.iter().flatten().copied().collect();
            let all_equal = present
                .windows(2)
                .all(|w| w[0].size == w[1].size && w[0].hash == w[1].hash);
            let status = if !all_equal {
                STATUS_DIFFERENT
            } else if present.len() < listings.len() {
                STATUS_MISSING
            } else {
                STATUS_SAME
            };
            let names = |want: bool| {
                listings
                    .iter()
                    .zip(&slots)
                    .filter(|(_, s)| s.is_some() == want)
                    .map(|(l, _)| l.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let mut row = vec![
                CellValue::String(path.to_string()),
                CellValue::String(status.to_string()),
                CellValue::String(names(true)),
                CellValue::String(names(false)),
            ];
            for slot in &slots {
                match slot {
                    Some(f) => {
                        row.push(CellValue::Int(f.size as i64));
                        row.push(match &f.modified {
                            Some(m) => {
                                CellValue::DateTime(m.format("%Y-%m-%d %H:%M:%S").to_string())
                            }
                            None => CellValue::Null,
                        });
                        row.push(
                            f.hash
                                .as_ref()
                                .map(|h| CellValue::String(hex(h)))
                                .unwrap_or(CellValue::Null),
                        );
                    }
                    None => row.extend([CellValue::Null, CellValue::Null, CellValue::Null]),
                }
            }
            row
        })
        .collect();
    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.format_name = Some("Folder comparison".to_string());
    table
}

/// Pairs of folders (by index into `labels`) that hold the file on `row`.
pub fn pairs_for_row(table: &DataTable, row: usize, labels: &[String]) -> Vec<(usize, usize)> {
    let present: Vec<usize> = labels
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            table
                .columns
                .iter()
                .position(|c| c.name == format!("size: {l}"))
                .and_then(|c| table.get(row, c))
                .is_some_and(|v| !matches!(v, CellValue::Null))
        })
        .map(|(i, _)| i)
        .collect();
    let mut pairs = Vec::new();
    for (k, &a) in present.iter().enumerate() {
        for &b in &present[k + 1..] {
            pairs.push((a, b));
        }
    }
    pairs
}

/// Whether any row is not `same` (the CLI's exit code).
pub fn any_difference(table: &DataTable) -> bool {
    (0..table.row_count()).any(|r| {
        table
            .get(r, 1)
            .is_some_and(|v| !matches!(v, CellValue::String(s) if s == STATUS_SAME))
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
