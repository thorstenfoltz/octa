//! Delta Lake / Apache Iceberg / plain parts-directory reader (read-only).
//!
//! Delta and Iceberg are *table formats*: a directory of Parquet data files
//! plus a transaction log / metadata layer that records which files form the
//! current snapshot, the schema, and its history. The unit you open is the
//! **directory**, not a single file, so this reader is driven from the
//! directory-open path in `src/app/file_io.rs` rather than the extension-based
//! registry (it is not registered as a normal `FormatReader`).
//!
//! A directory with no lakehouse markers but tabular *part files* inside
//! (`part-0.parquet`, `year=2024/month=03/part-1.csv.gz`, ...) opens as a
//! **dataset**: one DuckDB glob scan over every part, with Hive-style
//! `key=value` path segments materialised as real columns
//! (`hive_partitioning`) and drifted part schemas reconciled by column name
//! (`union_by_name`).
//!
//! Reading is delegated to the bundled DuckDB. Delta/Iceberg go via its
//! `delta` / `iceberg` extensions (`delta_scan('dir')` / `iceberg_scan('dir')`);
//! the extensions are **installed on first use, which needs network access**;
//! once cached by DuckDB they work offline. Datasets use the built-in
//! `read_parquet` / `read_csv` / `read_json` scanners (no extension, no
//! network). The directory must be complete; a lone `.parquet` extracted from
//! such a table should be opened with the normal Parquet reader instead.

use std::path::Path;

use anyhow::{Context, Result, bail};
use duckdb::Connection;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::duckdb_reader::{duckdb_type_to_arrow, duckdb_value_to_cell};

/// Which tabular file family a parts directory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartsFamily {
    Parquet,
    Delimited,
    JsonLines,
}

impl PartsFamily {
    /// Human-readable label used in `format_name`.
    pub fn label(self) -> &'static str {
        match self {
            PartsFamily::Parquet => "Parquet",
            PartsFamily::Delimited => "CSV",
            PartsFamily::JsonLines => "JSONL",
        }
    }

    /// Classify a file name into a family by its (possibly compressed)
    /// extension. `None` for non-data files (`_SUCCESS`, `README.md`, ...).
    fn of_file_name(name: &str) -> Option<PartsFamily> {
        let lower = name.to_lowercase();
        let stem = lower
            .strip_suffix(".gz")
            .or_else(|| lower.strip_suffix(".zst"))
            .unwrap_or(&lower);
        if stem.ends_with(".parquet") {
            // Compressed parquet part files are not a thing DuckDB globs;
            // only the plain extension counts.
            if lower.ends_with(".parquet") {
                return Some(PartsFamily::Parquet);
            }
            return None;
        }
        if stem.ends_with(".csv") || stem.ends_with(".tsv") {
            return Some(PartsFamily::Delimited);
        }
        if stem.ends_with(".jsonl") || stem.ends_with(".ndjson") {
            return Some(PartsFamily::JsonLines);
        }
        None
    }
}

/// Which lakehouse table format a directory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LakehouseKind {
    Delta,
    Iceberg,
    /// A plain directory of tabular part files (a "dataset"), no transaction
    /// log. Read via a DuckDB glob scan with Hive partitioning.
    Parts(PartsFamily),
}

impl LakehouseKind {
    /// Human-readable `format_name` stamped on the loaded table.
    pub fn format_name(self) -> String {
        match self {
            LakehouseKind::Delta => "Delta Lake".to_string(),
            LakehouseKind::Iceberg => "Apache Iceberg".to_string(),
            LakehouseKind::Parts(f) => format!("Dataset ({})", f.label()),
        }
    }
}

/// Maximum directory depth the parts scan descends (guards against
/// pathological trees; Hive layouts are rarely more than a few levels).
const PARTS_SCAN_MAX_DEPTH: usize = 8;

/// Recursively collect data files under `dir` grouped by family, as paths
/// relative to `dir`. Symlinks are not followed.
fn scan_parts(dir: &Path) -> Vec<(PartsFamily, String)> {
    fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<(PartsFamily, String)>) {
        if depth > PARTS_SCAN_MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if ft.is_dir() {
                walk(&entry.path(), &child_rel, depth + 1, out);
            } else if ft.is_file()
                && let Some(family) = PartsFamily::of_file_name(&name)
            {
                out.push((family, child_rel));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", 0, &mut out);
    out
}

/// Pick the majority family (ties: Parquet > Delimited > JsonLines) and the
/// list of files belonging to *other* families (the "skipped" report).
fn majority_family(files: &[(PartsFamily, String)]) -> Option<(PartsFamily, Vec<String>)> {
    if files.is_empty() {
        return None;
    }
    // Counts in tie-break priority order: the first entry with the maximum
    // count wins (Parquet > Delimited > JsonLines).
    let count = |f: PartsFamily| files.iter().filter(|(fam, _)| *fam == f).count();
    let counts = [
        (PartsFamily::Parquet, count(PartsFamily::Parquet)),
        (PartsFamily::Delimited, count(PartsFamily::Delimited)),
        (PartsFamily::JsonLines, count(PartsFamily::JsonLines)),
    ];
    let best = counts.iter().map(|(_, n)| *n).max()?;
    let winner = counts.iter().find(|(_, n)| *n == best).map(|(f, _)| *f)?;
    let skipped = files
        .iter()
        .filter(|(fam, _)| *fam != winner)
        .map(|(_, p)| p.clone())
        .collect();
    Some((winner, skipped))
}

/// Detect what `dir` holds: a Delta / Iceberg table (by marker subdirectory)
/// or a plain parts dataset (by the data files inside). Returns `None` for
/// ordinary directories with no tabular content. Markers win over parts since
/// a Delta table *also* contains bare parquet files.
pub fn detect(dir: &Path) -> Option<LakehouseKind> {
    if !dir.is_dir() {
        return None;
    }
    if dir.join("_delta_log").is_dir() {
        return Some(LakehouseKind::Delta);
    }
    if dir.join("metadata").is_dir() {
        return Some(LakehouseKind::Iceberg);
    }
    let files = scan_parts(dir);
    let (family, _) = majority_family(&files)?;
    Some(LakehouseKind::Parts(family))
}

/// Read a lakehouse table / dataset directory into a `DataTable` via DuckDB,
/// discarding the skipped-files report. Honours `initial_load_rows`.
pub fn read_dir(dir: &Path, kind: LakehouseKind) -> Result<DataTable> {
    read_dir_report(dir, kind).map(|(t, _)| t)
}

/// Like [`read_dir`], but also reports the data files a dataset scan skipped
/// because they belong to a minority family (always empty for Delta/Iceberg).
pub fn read_dir_report(dir: &Path, kind: LakehouseKind) -> Result<(DataTable, Vec<String>)> {
    let dir_str = dir
        .to_str()
        .with_context(|| format!("non-UTF-8 path {}", dir.display()))?;
    // DuckDB string literals: single-quote, internal quotes doubled.
    let dir_lit = dir_str.replace('\'', "''");

    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    let mut skipped = Vec::new();
    let scan = match kind {
        LakehouseKind::Delta | LakehouseKind::Iceberg => {
            // Install + load the extension. INSTALL reaches the network on
            // first use; surface a clear error rather than a raw DuckDB
            // message if it fails.
            let (ext, scan_fn) = match kind {
                LakehouseKind::Delta => ("delta", "delta_scan"),
                _ => ("iceberg", "iceberg_scan"),
            };
            conn.execute_batch(&format!("INSTALL {ext}; LOAD {ext};"))
                .with_context(|| {
                    format!(
                        "loading the DuckDB '{ext}' extension (first use needs network access to install it)"
                    )
                })?;
            format!("{scan_fn}('{dir_lit}')")
        }
        LakehouseKind::Parts(family) => {
            let files = scan_parts(dir);
            let Some((detected, skip)) = majority_family(&files) else {
                bail!("no tabular part files found in {}", dir.display());
            };
            // Trust the freshly scanned majority over a stale `kind` (the
            // directory may have changed since detection).
            let family = if detected == family { family } else { detected };
            skipped = skip;
            // Pass the exact scanned file list (not globs): DuckDB errors on
            // a glob that matches nothing, and the explicit list scans
            // precisely the majority-family files the report accounts for.
            let paths: Vec<String> = files
                .iter()
                .filter(|(fam, _)| *fam == family)
                .map(|(_, rel)| format!("'{dir_lit}/{}'", rel.replace('\'', "''")))
                .collect();
            let list = format!("[{}]", paths.join(", "));
            match family {
                PartsFamily::Parquet => {
                    format!("read_parquet({list}, hive_partitioning = true, union_by_name = true)")
                }
                PartsFamily::Delimited => {
                    format!("read_csv({list}, hive_partitioning = true, union_by_name = true)")
                }
                PartsFamily::JsonLines => format!(
                    "read_json({list}, format = 'newline_delimited', \
                     hive_partitioning = true, union_by_name = true)"
                ),
            }
        }
    };

    let columns = describe_columns(&conn, &scan)?;
    if columns.is_empty() {
        bail!("the {} table has no columns", kind.format_name());
    }

    let max_rows = super::initial_load_rows();
    let sql = format!("SELECT * FROM {scan} LIMIT {max_rows}");
    let mut stmt = conn
        .prepare(&sql)
        .with_context(|| format!("scanning {} table {}", kind.format_name(), dir.display()))?;

    let col_count = columns.len();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(col_count);
        for i in 0..col_count {
            row.push(duckdb_value_to_cell(r.get_ref(i)?));
        }
        rows.push(row);
    }

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(dir.to_string_lossy().to_string());
    table.format_name = Some(kind.format_name());
    Ok((table, skipped))
}

/// Column names + Octa types from `DESCRIBE SELECT * FROM <scan>`.
fn describe_columns(conn: &Connection, scan: &str) -> Result<Vec<ColumnInfo>> {
    let mut stmt = conn.prepare(&format!("DESCRIBE SELECT * FROM {scan}"))?;
    let cols = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let ty: String = r.get(1)?;
            Ok(ColumnInfo {
                name,
                data_type: duckdb_type_to_arrow(&ty),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_delta_by_log_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("_delta_log")).unwrap();
        assert_eq!(detect(dir.path()), Some(LakehouseKind::Delta));
    }

    #[test]
    fn detects_iceberg_by_metadata_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("metadata")).unwrap();
        assert_eq!(detect(dir.path()), Some(LakehouseKind::Iceberg));
    }

    #[test]
    fn plain_directory_is_not_a_lakehouse() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data")).unwrap();
        assert_eq!(detect(dir.path()), None);
    }

    #[test]
    fn a_file_is_not_a_lakehouse() {
        let f = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(detect(f.path()), None);
    }

    #[test]
    fn detects_parts_dir_of_csvs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("year=2024")).unwrap();
        std::fs::write(dir.path().join("year=2024/p0.csv"), "a,b\n1,2\n").unwrap();
        std::fs::write(dir.path().join("year=2024/p1.csv"), "a,b\n3,4\n").unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(LakehouseKind::Parts(PartsFamily::Delimited))
        );
    }

    #[test]
    fn delta_marker_wins_over_parts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("_delta_log")).unwrap();
        std::fs::write(dir.path().join("p0.parquet"), b"junk").unwrap();
        assert_eq!(detect(dir.path()), Some(LakehouseKind::Delta));
    }

    #[test]
    fn reads_csv_parts_with_hive_column() {
        let dir = tempfile::tempdir().unwrap();
        for (y, v) in [("2023", "1"), ("2024", "2")] {
            let sub = dir.path().join(format!("year={y}"));
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("part.csv"), format!("a\n{v}\n")).unwrap();
        }
        let (t, skipped) =
            read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Delimited)).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(t.row_count(), 2);
        assert!(
            t.columns.iter().any(|c| c.name == "year"),
            "hive col materialised: {:?}",
            t.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn majority_family_wins_and_reports_skips() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.csv"), "x\n1\n").unwrap();
        std::fs::write(dir.path().join("b.csv"), "x\n2\n").unwrap();
        std::fs::write(dir.path().join("c.jsonl"), "{\"x\":3}\n").unwrap();
        let Some(LakehouseKind::Parts(fam)) = detect(dir.path()) else {
            panic!("expected a parts dir");
        };
        assert_eq!(fam, PartsFamily::Delimited);
        let (t, skipped) = read_dir_report(dir.path(), LakehouseKind::Parts(fam)).unwrap();
        assert_eq!(t.row_count(), 2);
        assert_eq!(skipped, vec!["c.jsonl".to_string()]);
    }

    #[test]
    fn reads_gzipped_csv_parts() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let f = std::fs::File::create(dir.path().join("p0.csv.gz")).unwrap();
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        enc.write_all(b"a\n7\n").unwrap();
        enc.finish().unwrap();
        assert_eq!(
            detect(dir.path()),
            Some(LakehouseKind::Parts(PartsFamily::Delimited))
        );
        let (t, _) =
            read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Delimited)).unwrap();
        assert_eq!(t.row_count(), 1);
    }

    #[test]
    fn jsonl_parts_read_as_dataset() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("p0.jsonl"), "{\"x\": 1}\n{\"x\": 2}\n").unwrap();
        let (t, _) =
            read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::JsonLines)).unwrap();
        assert_eq!(t.row_count(), 2);
        assert!(t.columns.iter().any(|c| c.name == "x"));
    }

    #[test]
    fn parquet_wins_family_tie() {
        let files = vec![
            (PartsFamily::JsonLines, "a.jsonl".to_string()),
            (PartsFamily::Parquet, "b.parquet".to_string()),
        ];
        let (winner, skipped) = majority_family(&files).unwrap();
        assert_eq!(winner, PartsFamily::Parquet);
        assert_eq!(skipped, vec!["a.jsonl".to_string()]);
    }

    #[test]
    fn empty_dataset_dir_errors_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_dir_report(dir.path(), LakehouseKind::Parts(PartsFamily::Parquet))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no tabular part files"), "{err}");
    }
}
