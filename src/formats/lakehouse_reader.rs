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

use crate::data::recipe::unique_name;
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::duckdb_reader::{
    column_zones, duckdb_session_timezone, duckdb_type_to_arrow, duckdb_value_to_cell_tz,
};
use crate::formats::lakehouse_history::VersionId;

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
    let dir_lit = dir_literal(dir)?;
    let conn = open_connection(kind)?;
    let mut skipped = Vec::new();
    let scan = match kind {
        LakehouseKind::Delta | LakehouseKind::Iceberg => table_scan(&dir_lit, kind, None)?,
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
    let table = read_scan(&conn, &scan, dir, kind)?;
    Ok((table, skipped))
}

/// An in-memory DuckDB with the extension `kind` reads through, loaded.
/// Public so the GUI can take the connection's interrupt handle before a
/// long comparison starts on a worker thread.
pub fn open_connection(kind: LakehouseKind) -> Result<Connection> {
    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    let ext = match kind {
        LakehouseKind::Delta => "delta",
        LakehouseKind::Iceberg => "iceberg",
        LakehouseKind::Parts(_) => return Ok(conn),
    };
    // INSTALL reaches the network on first use; surface a clear error rather
    // than a raw DuckDB message if it fails.
    conn.execute_batch(&format!("INSTALL {ext}; LOAD {ext};"))
        .with_context(|| {
            format!("loading the DuckDB '{ext}' extension (first use needs network access to install it)")
        })?;
    Ok(conn)
}

/// `dir` as the body of a DuckDB string literal (internal quotes doubled).
fn dir_literal(dir: &Path) -> Result<String> {
    let s = dir
        .to_str()
        .with_context(|| format!("non-UTF-8 path {}", dir.display()))?;
    Ok(s.replace('\'', "''"))
}

/// The table function reading a Delta / Iceberg folder at `version`, or the
/// latest. Iceberg passes `allow_moved_paths`: its metadata holds absolute
/// paths, so a table copied or downloaded elsewhere would not open without it.
fn table_scan(dir_lit: &str, kind: LakehouseKind, version: Option<&VersionId>) -> Result<String> {
    Ok(match (kind, version) {
        (LakehouseKind::Delta, None) => format!("delta_scan('{dir_lit}')"),
        (LakehouseKind::Delta, Some(VersionId::Delta(n))) => {
            format!("delta_scan('{dir_lit}', version = {n})")
        }
        (LakehouseKind::Iceberg, None) => {
            format!("iceberg_scan('{dir_lit}', allow_moved_paths = true)")
        }
        (LakehouseKind::Iceberg, Some(VersionId::Iceberg { snapshot_id, .. })) => format!(
            "iceberg_scan('{dir_lit}', allow_moved_paths = true, snapshot_from_id = {snapshot_id})"
        ),
        (kind, Some(v)) => bail!("{v:?} is not a version of a {} table", kind.format_name()),
        (LakehouseKind::Parts(_), None) => {
            bail!("a folder of part files is read with read_parquet / read_csv, not a table scan")
        }
    })
}

/// `SELECT * FROM {scan}` up to the row cap, stamped as a table of `dir`.
fn read_scan(conn: &Connection, scan: &str, dir: &Path, kind: LakehouseKind) -> Result<DataTable> {
    let columns = describe_columns(conn, scan)?;
    if columns.is_empty() {
        bail!("the {} table has no columns", kind.format_name());
    }
    let max_rows = super::initial_load_rows().min(i64::MAX as usize);
    let rows = fetch_rows(
        conn,
        &format!("SELECT * FROM {scan} LIMIT {max_rows}"),
        &columns,
    )
    .with_context(|| format!("scanning {} table {}", kind.format_name(), dir.display()))?;
    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(dir.to_string_lossy().to_string());
    table.format_name = Some(kind.format_name());
    Ok(table)
}

/// Run `sql` and convert every row with the types in `columns`.
fn fetch_rows(conn: &Connection, sql: &str, columns: &[ColumnInfo]) -> Result<Vec<Vec<CellValue>>> {
    let mut stmt = conn.prepare(sql)?;
    let zones = column_zones(columns);
    let mut rows = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(zones.len());
        for (i, tz) in zones.iter().enumerate() {
            row.push(duckdb_value_to_cell_tz(r.get_ref(i)?, tz.as_deref()));
        }
        rows.push(row);
    }
    Ok(rows)
}

/// One version of a Delta / Iceberg table, read like opening the table (same
/// row cap, same types). The caller clears `source_path`: a version is a
/// copy, and Refresh must not jump it to the latest.
pub fn read_version(dir: &Path, kind: LakehouseKind, version: &VersionId) -> Result<DataTable> {
    let scan = table_scan(&dir_literal(dir)?, kind, Some(version))?;
    let conn = open_connection(kind)?;
    read_scan(&conn, &scan, dir, kind).map_err(explain_missing_files)
}

/// What changed between two versions, computed by DuckDB over both versions
/// in full: rows only in `newer` are added, rows only in `older` removed
/// (`EXCEPT ALL`, so duplicates count). An updated row is one of each.
pub struct VersionDiff {
    /// A `change` column (`added` / `removed`), then the shared columns in
    /// the newer version's order; at most the row cap rows.
    pub table: DataTable,
    /// Exact totals, however many rows `table` shows.
    pub added: u64,
    pub removed: u64,
    /// Columns in only one version, left out of the comparison.
    pub only_in_older: Vec<String>,
    pub only_in_newer: Vec<String>,
}

pub fn diff_versions(
    conn: &Connection,
    dir: &Path,
    kind: LakehouseKind,
    older: &VersionId,
    newer: &VersionId,
    row_cap: usize,
) -> Result<VersionDiff> {
    let dir_lit = dir_literal(dir)?;
    let old_scan = table_scan(&dir_lit, kind, Some(older))?;
    let new_scan = table_scan(&dir_lit, kind, Some(newer))?;
    let old_cols = describe_columns(conn, &old_scan).map_err(explain_missing_files)?;
    let new_cols = describe_columns(conn, &new_scan).map_err(explain_missing_files)?;
    let in_old = |name: &str| old_cols.iter().any(|c| c.name == name);
    let in_new = |name: &str| new_cols.iter().any(|c| c.name == name);
    let shared: Vec<ColumnInfo> = new_cols
        .iter()
        .filter(|c| in_old(&c.name))
        .cloned()
        .collect();
    if shared.is_empty() {
        bail!("the two versions have no column in common");
    }
    let only_in_older = old_cols
        .iter()
        .filter(|c| !in_new(&c.name))
        .map(|c| c.name.clone())
        .collect();
    let only_in_newer = new_cols
        .iter()
        .filter(|c| !in_old(&c.name))
        .map(|c| c.name.clone())
        .collect();

    let list = shared
        .iter()
        .map(|c| crate::sql::quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let added_sql =
        format!("SELECT {list} FROM {new_scan} EXCEPT ALL SELECT {list} FROM {old_scan}");
    let removed_sql =
        format!("SELECT {list} FROM {old_scan} EXCEPT ALL SELECT {list} FROM {new_scan}");
    let count = |sql: &str| -> Result<u64> {
        let n: i64 = conn.query_row(&format!("SELECT count(*) FROM ({sql})"), [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    };
    let added = count(&added_sql).map_err(explain_missing_files)?;
    let removed = count(&removed_sql).map_err(explain_missing_files)?;

    let names: Vec<String> = shared.iter().map(|c| c.name.clone()).collect();
    let mut columns = vec![ColumnInfo {
        name: unique_name(&names, "change"),
        data_type: "Utf8".to_string(),
    }];
    columns.extend(shared);
    let cap = row_cap.min(i64::MAX as usize);
    let rows_sql = format!(
        "SELECT * FROM (SELECT 'added', * FROM ({added_sql}) \
         UNION ALL SELECT 'removed', * FROM ({removed_sql})) ORDER BY 1 LIMIT {cap}"
    );
    let rows = fetch_rows(conn, &rows_sql, &columns).map_err(explain_missing_files)?;
    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    Ok(VersionDiff {
        table,
        added,
        removed,
        only_in_older,
        only_in_newer,
    })
}

/// A version whose data files were deleted (VACUUM, a cleaned-up table)
/// fails deep in DuckDB with "Cannot open file ... No such file or
/// directory" (seen 2026-10-08). Say what that means, keep the detail.
fn explain_missing_files(e: anyhow::Error) -> anyhow::Error {
    let text = format!("{e:#}");
    if text.contains("Cannot open file") || text.contains("No such file") {
        e.context(crate::i18n::t("dialog.versions_lake_vacuumed"))
    } else {
        e
    }
}

/// Column names + Octa types from `DESCRIBE SELECT * FROM <scan>`.
fn describe_columns(conn: &Connection, scan: &str) -> Result<Vec<ColumnInfo>> {
    let session_tz = duckdb_session_timezone(conn);
    let mut stmt = conn.prepare(&format!("DESCRIBE SELECT * FROM {scan}"))?;
    let cols = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let ty: String = r.get(1)?;
            Ok(ColumnInfo {
                name,
                data_type: duckdb_type_to_arrow(&ty, session_tz.as_deref()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

#[cfg(test)]
#[path = "lakehouse_reader_tests.rs"]
mod tests;
