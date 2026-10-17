use crate::data::{CellValue, DataTable};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// One DataTable per distinct value of `col`. Null groups under "__null__".
/// Where the partition files land.
///
/// Two independent questions - is there a folder per value, and is the file
/// named after the value or numbered - would be a two-by-two matrix to
/// explain. Presenting the four useful combinations as one list instead is
/// what makes the dialog readable: the user picks a shape, not two switches.
///
/// `Flat` is the original behaviour and stays the default. All four write the
/// same rows, and all four can be reopened as one table (the partition column
/// is written into every file), so the choice is only about the names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartitionLayout {
    /// `new_york.csv` - one file per value, named after it.
    #[default]
    Flat,
    /// `New York/part-0001.csv` - a folder per value, numbered file inside.
    Folder,
    /// `city=New York/data.csv` - the lakehouse convention.
    Hive,
    /// `city=New York/part-0001.csv` - the lakehouse convention, with the
    /// part-file naming Spark and friends actually write.
    HiveParts,
}

impl PartitionLayout {
    /// Every layout, in the order the dialog lists them.
    pub const ALL: &'static [PartitionLayout] =
        &[Self::Flat, Self::Folder, Self::Hive, Self::HiveParts];

    /// The CLI / MCP word for this layout.
    pub fn id(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Folder => "folder",
            Self::Hive => "hive",
            Self::HiveParts => "hive-parts",
        }
    }

    /// i18n key for the radio label.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Flat => "partition.layout_flat",
            Self::Folder => "partition.layout_folder",
            Self::Hive => "partition.layout_hive",
            Self::HiveParts => "partition.layout_hive_parts",
        }
    }

    /// i18n key for the tooltip.
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Flat => "partition.layout_flat_hint",
            Self::Folder => "partition.layout_folder_hint",
            Self::Hive => "partition.layout_hive_hint",
            Self::HiveParts => "partition.layout_hive_parts_hint",
        }
    }

    /// Parse the CLI / MCP word. `None` for anything else, so the caller can
    /// name the valid words in its error. `hive_parts` is accepted alongside
    /// `hive-parts` because both spellings are natural on a command line.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_ascii_lowercase().replace('_', "-");
        Self::ALL.iter().copied().find(|l| l.id() == t)
    }
}

/// Replace only what cannot appear in a path component.
///
/// Deliberately not `sanitize_sql_name`, which the flat layout uses for file
/// stems: a value that lands in a directory name has to survive being read
/// back out of it, so dots, dashes, spaces and case are kept as they are.
fn path_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// `<col>=<value>` for one Hive partition directory.
pub fn hive_dir_name(col: &str, value: &str) -> String {
    format!("{}={}", path_safe(col), path_safe(value))
}

/// The path one partition is written to, relative to the output folder, when
/// no row or size limit is set.
///
/// **The single source of truth for partition naming.** The writer joins this
/// onto the output directory and the dialog shows it as a preview, so what is
/// on screen before you press Apply is literally what lands on disk. The
/// numbered layouts count files per folder, so without a limit every folder
/// holds `part-0001`.
pub fn partition_path(layout: PartitionLayout, col_name: &str, value: &str, ext: &str) -> String {
    match layout {
        PartitionLayout::Flat => format!("{}.{ext}", crate::sql::sanitize_sql_name(value)),
        PartitionLayout::Folder => format!("{}/part-0001.{ext}", path_safe(value)),
        PartitionLayout::Hive => format!("{}/data.{ext}", hive_dir_name(col_name, value)),
        PartitionLayout::HiveParts => {
            format!("{}/part-0001.{ext}", hive_dir_name(col_name, value))
        }
    }
}

/// Part `part` (1-based) of the file `base_rel` names, once a row or size
/// limit splits it. Flat files keep their stem (`berlin_part_0001.csv`); the
/// folder layouts number the files inside their folder (`part-0002.csv`),
/// which also turns Hive's single `data.csv` into numbered parts.
pub fn part_path(layout: PartitionLayout, base_rel: &str, ext: &str, part: usize) -> String {
    match layout {
        PartitionLayout::Flat => {
            let stem = base_rel
                .strip_suffix(&format!(".{ext}"))
                .unwrap_or(base_rel);
            format!("{stem}_part_{part:04}.{ext}")
        }
        _ => {
            let dir = base_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            format!("{dir}/part-{part:04}.{ext}")
        }
    }
}

/// Disambiguate a `Flat` name that two different values sanitise down to.
///
/// Only `Flat` needs this: it is the one layout whose naming is lossy, since
/// `sanitize_sql_name` folds case and punctuation, so `New York`, `new-york`
/// and `NEW_YORK` all arrive as `new_york`. The folder layouts keep the value
/// path-safe but otherwise intact, so their names are already distinct.
pub fn dedupe_flat_name(
    layout: PartitionLayout,
    rel: String,
    ext: &str,
    seen: &mut std::collections::HashMap<String, usize>,
) -> String {
    if layout != PartitionLayout::Flat {
        return rel;
    }
    let stem = rel.trim_end_matches(&format!(".{ext}")).to_string();
    let n = seen.entry(stem.clone()).or_insert(0);
    *n += 1;
    if *n == 1 {
        rel
    } else {
        format!("{stem}_{n}.{ext}")
    }
}

/// Row indices per distinct value of `col`, value-sorted; row order within a
/// group is preserved. Null groups under "__null__".
fn group_rows(table: &DataTable, col: usize) -> Vec<(String, Vec<usize>)> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for row in 0..table.row_count() {
        let key = match table.get(row, col) {
            Some(CellValue::Null) | None => "__null__".to_string(),
            Some(v) => v.to_string(),
        };
        groups.entry(key).or_default().push(row);
    }
    groups.into_iter().collect()
}

/// The rows at `idxs` as their own table (edits applied, columns kept).
fn take_rows(table: &DataTable, idxs: &[usize]) -> DataTable {
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = idxs
        .iter()
        .map(|&r| {
            (0..table.col_count())
                .map(|c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    out.structural_changes = true;
    out
}

/// Row order within a group is preserved; groups are returned value-sorted.
pub fn partition_table(table: &DataTable, col: usize) -> Vec<(String, DataTable)> {
    group_rows(table, col)
        .into_iter()
        .map(|(value, idxs)| (value, take_rows(table, &idxs)))
        .collect()
}

/// What to split by. At least one of the three limits is set
/// ([`SplitSpec::validate`]); they combine: by column, then each group
/// chunked by rows and/or size.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplitSpec {
    /// Split by this column's distinct values first.
    pub column: Option<usize>,
    /// Naming when `column` is set; ignored without one.
    pub layout: PartitionLayout,
    /// At most this many rows per file.
    pub max_rows: Option<usize>,
    /// At most this many bytes per file. Parts land a little under it.
    pub max_bytes: Option<u64>,
}

impl SplitSpec {
    /// Whether files are chunked, which numbers every one of them.
    pub fn is_limited(&self) -> bool {
        self.max_rows.is_some() || self.max_bytes.is_some()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.column.is_none() && !self.is_limited() {
            anyhow::bail!("choose a column, a row limit or a size limit to split by");
        }
        if self.max_rows == Some(0) {
            anyhow::bail!("the row limit must be at least 1");
        }
        if self.max_bytes == Some(0) {
            anyhow::bail!("the size limit must be more than 0 bytes");
        }
        Ok(())
    }
}

/// One file a split wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenPart {
    /// The column value of its group, `None` when not split by a column.
    pub value: Option<String>,
    pub path: PathBuf,
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SplitOutcome {
    pub parts: Vec<WrittenPart>,
    /// Stopped by the caller's cancel flag; `parts` holds what was written.
    pub cancelled: bool,
}

/// Rows written for the first size measurement of a group.
const TRIAL_ROWS: usize = 1_000;
/// Aim under the limit so the re-measure rarely has to rewrite.
const SIZE_MARGIN: f64 = 0.95;

/// The base name (no part number) of one group's output.
fn base_rel(
    spec: &SplitSpec,
    col_name: &str,
    value: Option<&str>,
    source_stem: &str,
    ext: &str,
    seen: &mut HashMap<String, usize>,
) -> (PartitionLayout, String) {
    match value {
        Some(v) => {
            let rel = partition_path(spec.layout, col_name, v, ext);
            (spec.layout, dedupe_flat_name(spec.layout, rel, ext, seen))
        }
        None => (
            PartitionLayout::Flat,
            format!("{}.{ext}", path_safe(source_stem)),
        ),
    }
}

/// What the dialog shows: the first file each of `values` would get (or the
/// whole table's first file without a column), named exactly as
/// [`write_split`] names them.
pub fn preview_paths(
    spec: &SplitSpec,
    col_name: &str,
    values: &[String],
    source_stem: &str,
    ext: &str,
) -> Vec<String> {
    let mut seen = HashMap::new();
    let one = |value: Option<&str>, seen: &mut HashMap<String, usize>| {
        let (layout, base) = base_rel(spec, col_name, value, source_stem, ext, seen);
        if spec.is_limited() {
            part_path(layout, &base, ext, 1)
        } else {
            base
        }
    };
    if spec.column.is_none() {
        return vec![one(None, &mut seen)];
    }
    values.iter().map(|v| one(Some(v), &mut seen)).collect()
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn rows_for(limit: u64, bytes_per_row: f64) -> usize {
    ((limit as f64 / bytes_per_row.max(1e-9)) * SIZE_MARGIN)
        .floor()
        .max(1.0) as usize
}

/// The caller's side of a split: how a file is written, how to stop, and
/// what to tell the user after each file. Bundled so `write_split` stays
/// under clippy's argument limit without silencing it.
pub struct SplitIo<'a> {
    pub write: &'a mut dyn FnMut(&DataTable, &Path) -> anyhow::Result<()>,
    pub cancel: &'a AtomicBool,
    pub progress: &'a mut dyn FnMut(usize, &Path),
}

/// Split `table` per `spec` into `out_dir`, writing each file with `io.write`.
///
/// The one code path behind the dialog, `--partition-by` / `--split-rows` /
/// `--split-size` and the `partition_table` tool. A size limit is met by
/// writing a part, measuring it, and rewriting it smaller when it came out
/// too big, which works for every format including compressed Parquet whose
/// size cannot be predicted.
///
/// ponytail: parts land near the limit, not packed to the byte; a binary
/// search over the row count would pack them if anyone ever needs that.
pub fn write_split(
    table: &DataTable,
    spec: &SplitSpec,
    out_dir: &Path,
    ext: &str,
    source_stem: &str,
    io: SplitIo<'_>,
) -> anyhow::Result<SplitOutcome> {
    spec.validate()?;
    let SplitIo {
        write,
        cancel,
        progress,
    } = io;
    let col_name = spec
        .column
        .and_then(|c| table.columns.get(c))
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let groups: Vec<(Option<String>, Vec<usize>)> = match spec.column {
        Some(c) => group_rows(table, c)
            .into_iter()
            .map(|(v, idxs)| (Some(v), idxs))
            .collect(),
        None => vec![(None, (0..table.row_count()).collect())],
    };

    let mut seen = HashMap::new();
    let mut out = SplitOutcome::default();
    for (value, idxs) in groups {
        let (layout, base) = base_rel(
            spec,
            &col_name,
            value.as_deref(),
            source_stem,
            ext,
            &mut seen,
        );
        let mut part = 0usize;
        let mut start = 0usize;
        let mut bytes_per_row: Option<f64> = None;
        loop {
            if cancel.load(Ordering::Relaxed) {
                out.cancelled = true;
                return Ok(out);
            }
            part += 1;
            let rel = if spec.is_limited() {
                part_path(layout, &base, ext, part)
            } else {
                base.clone()
            };
            let path = out_dir.join(&rel);
            ensure_parent(&path)?;
            let remaining = idxs.len() - start;
            let mut n = remaining.min(spec.max_rows.unwrap_or(usize::MAX));
            match spec.max_bytes {
                None => write(&take_rows(table, &idxs[start..start + n]), &path)?,
                Some(limit) => {
                    if let Some(bpr) = bytes_per_row {
                        n = n.min(rows_for(limit, bpr));
                    } else if n > TRIAL_ROWS {
                        write(&take_rows(table, &idxs[start..start + TRIAL_ROWS]), &path)?;
                        let size = std::fs::metadata(&path)?.len();
                        n = n.min(rows_for(limit, size as f64 / TRIAL_ROWS as f64));
                    }
                    loop {
                        write(&take_rows(table, &idxs[start..start + n]), &path)?;
                        let size = std::fs::metadata(&path)?.len();
                        if size <= limit {
                            if n > 0 {
                                bytes_per_row = Some(size as f64 / n as f64);
                            }
                            break;
                        }
                        if n <= 1 {
                            let _ = std::fs::remove_file(&path);
                            if n == 0 {
                                anyhow::bail!(
                                    "an empty file is {size} bytes, over the size limit of {limit} bytes"
                                );
                            }
                            anyhow::bail!(
                                "the file holding only row {} is {size} bytes, over the size limit of {limit} bytes",
                                idxs[start] + 1
                            );
                        }
                        let shrunk = (n as f64 * (limit as f64 / size as f64) * SIZE_MARGIN).floor()
                            as usize;
                        n = shrunk.clamp(1, n - 1);
                    }
                }
            }
            out.parts.push(WrittenPart {
                value: value.clone(),
                path: path.clone(),
                rows: n,
            });
            progress(out.parts.len(), &path);
            start += n;
            if start >= idxs.len() {
                break;
            }
        }
    }
    Ok(out)
}

/// `25MB`, `500 KB`, `1.5GB`, `2MiB`, or plain bytes. KB/MB/GB are decimal
/// (1 MB = 1,000,000 bytes), the safe reading of an upload limit; KiB/MiB/GiB
/// are binary. Case and a space before the unit do not matter.
pub fn parse_size(s: &str) -> Result<u64, String> {
    let t = s.trim().to_ascii_lowercase();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let mult: f64 = match unit.trim() {
        "" | "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return Err(format!("unknown size unit in {s:?}; use KB, MB or GB")),
    };
    let n: f64 = num
        .parse()
        .map_err(|_| format!("{s:?} is not a size like 25MB"))?;
    let bytes = (n * mult).round();
    if bytes < 1.0 {
        return Err(format!("{s:?} must be more than 0 bytes"));
    }
    Ok(bytes as u64)
}

#[cfg(test)]
#[path = "partition_tests.rs"]
mod tests;
