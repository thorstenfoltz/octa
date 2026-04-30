//! GeoPackage (`.gpkg`) reader.
//!
//! GeoPackage is a SQLite database with a standardised schema: user data lives
//! in normal SQLite tables, while metadata (CRS, geometry columns, extensions,
//! tile matrices, …) lives in `gpkg_*` tables. We delegate read/write to
//! [`SqliteReader`] and only override [`FormatReader::list_tables`] to hide the
//! `gpkg_*` machinery so the table picker shows just the user's data tables.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

use crate::data::DataTable;

use super::sqlite_reader::SqliteReader;
use super::{FormatReader, TableInfo};

pub struct GeoPackageReader;

const FORMAT_NAME: &str = "GeoPackage";

impl FormatReader for GeoPackageReader {
    fn name(&self) -> &str {
        FORMAT_NAME
    }

    fn extensions(&self) -> &[&str] {
        &["gpkg"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let mut t = SqliteReader.read_file(path)?;
        t.format_name = Some(FORMAT_NAME.to_string());
        Ok(t)
    }

    fn supports_write(&self) -> bool {
        true
    }

    fn write_file(&self, path: &Path, table: &DataTable) -> Result<()> {
        SqliteReader.write_file(path, table)
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        // Prefer the GPKG-aware listing so the picker hides metadata tables.
        // If `gpkg_contents` is absent — i.e. the file has a `.gpkg` extension
        // but isn't a valid GeoPackage — fall through to SQLite's listing so
        // the user still gets *something* useful.
        match list_gpkg_data_tables(path)? {
            Some(tables) if !tables.is_empty() => Ok(Some(tables)),
            _ => SqliteReader.list_tables(path),
        }
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let mut t = SqliteReader.read_table(path, table)?;
        t.format_name = Some(FORMAT_NAME.to_string());
        Ok(t)
    }
}

/// Returns `Some(tables)` when `gpkg_contents` exists, `None` when the file
/// doesn't look like a GeoPackage. Tables of `data_type` `tiles` are excluded
/// because they hold raster blobs that aren't meaningfully tabular.
fn list_gpkg_data_tables(path: &Path) -> Result<Option<Vec<TableInfo>>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening GeoPackage at {}", path.display()))?;

    let has_contents: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='table' AND name='gpkg_contents'",
        [],
        |r| r.get(0),
    )?;
    if has_contents == 0 {
        return Ok(None);
    }

    let mut stmt = conn.prepare(
        "SELECT table_name FROM gpkg_contents \
         WHERE data_type IN ('features', 'attributes') \
         ORDER BY table_name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;

    let all = SqliteReader.list_tables(path)?.unwrap_or_default();
    let by_name: HashMap<&str, &TableInfo> =
        all.iter().map(|t| (t.name.as_str(), t)).collect();
    let filtered: Vec<TableInfo> = names
        .iter()
        .filter_map(|n| by_name.get(n.as_str()).map(|t| (*t).clone()))
        .collect();
    Ok(Some(filtered))
}
