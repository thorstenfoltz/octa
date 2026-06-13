//! Delta Lake / Apache Iceberg reader (read-only).
//!
//! Delta and Iceberg are *table formats*: a directory of Parquet data files
//! plus a transaction log / metadata layer that records which files form the
//! current snapshot, the schema, and its history. The unit you open is the
//! **directory**, not a single file, so this reader is driven from the
//! directory-open path in `src/app/file_io.rs` rather than the extension-based
//! registry (it is not registered as a normal `FormatReader`).
//!
//! Reading is delegated to the bundled DuckDB via its `delta` / `iceberg`
//! extensions (`delta_scan('dir')` / `iceberg_scan('dir')`), the same engine
//! the Parquet reader already falls back to. The extensions are **installed on
//! first use, which needs network access**; once cached by DuckDB they work
//! offline. The directory must be complete (log/metadata + every Parquet file
//! it references) - a lone `.parquet` extracted from such a table should be
//! opened with the normal Parquet reader instead.

use std::path::Path;

use anyhow::{Context, Result, bail};
use duckdb::Connection;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::duckdb_reader::{duckdb_type_to_arrow, duckdb_value_to_cell};

/// Which lakehouse table format a directory holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LakehouseKind {
    Delta,
    Iceberg,
}

impl LakehouseKind {
    /// DuckDB extension name + the table function it provides.
    fn extension(self) -> &'static str {
        match self {
            LakehouseKind::Delta => "delta",
            LakehouseKind::Iceberg => "iceberg",
        }
    }

    fn scan_fn(self) -> &'static str {
        match self {
            LakehouseKind::Delta => "delta_scan",
            LakehouseKind::Iceberg => "iceberg_scan",
        }
    }

    /// Human-readable `format_name` stamped on the loaded table.
    pub fn format_name(self) -> &'static str {
        match self {
            LakehouseKind::Delta => "Delta Lake",
            LakehouseKind::Iceberg => "Apache Iceberg",
        }
    }
}

/// Detect whether `dir` is a Delta or Iceberg table directory by looking for
/// the format's marker subdirectory: `_delta_log/` (Delta) or `metadata/`
/// (Iceberg). Returns `None` for ordinary directories. Delta is checked first
/// since `_delta_log` is unambiguous.
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
    None
}

/// Read a lakehouse table directory into a `DataTable` via DuckDB. Honours the
/// process-wide `initial_load_rows` cap (a `LIMIT` on the scan).
pub fn read_dir(dir: &Path, kind: LakehouseKind) -> Result<DataTable> {
    let dir_str = dir
        .to_str()
        .with_context(|| format!("non-UTF-8 path {}", dir.display()))?;
    // DuckDB string literals: single-quote, internal quotes doubled.
    let dir_lit = dir_str.replace('\'', "''");
    let scan = format!("{}('{}')", kind.scan_fn(), dir_lit);

    let conn = Connection::open_in_memory().context("opening in-memory DuckDB")?;
    // Install + load the extension. INSTALL reaches the network on first use;
    // surface a clear error rather than a raw DuckDB message if it fails.
    let ext = kind.extension();
    conn.execute_batch(&format!("INSTALL {ext}; LOAD {ext};"))
        .with_context(|| {
            format!(
                "loading the DuckDB '{ext}' extension (first use needs network access to install it)"
            )
        })?;

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
    table.format_name = Some(kind.format_name().to_string());
    Ok(table)
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
}
