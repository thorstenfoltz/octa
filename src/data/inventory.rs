//! Cloud bucket inventory: turn a recursive object listing into a plain
//! [`DataTable`] (one row per object, `find`-style path column first) so the
//! whole downstream surface (sort, filter, SQL, charts, export) works on it
//! unchanged. Pure; the listing itself happens in the cloud worker.

use crate::cloud::ObjectEntry;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// Split a key into (parent path, file name): `sub/deep/c.csv` ->
/// (`sub/deep`, `c.csv`); `a.csv` -> (``, `a.csv`).
fn split_key(key: &str) -> (String, String) {
    match key.rsplit_once('/') {
        Some((dir, name)) => (dir.to_string(), name.to_string()),
        None => (String::new(), key.to_string()),
    }
}

/// Lowercased extension of a file name (no leading dot), empty when none.
fn extension_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Build the inventory table from a flat (recursive) listing. Folder entries
/// are skipped defensively; recursive listings should not contain any.
pub fn build_inventory_table(entries: &[ObjectEntry]) -> DataTable {
    let col = |name: &str, ty: &str| ColumnInfo {
        name: name.to_string(),
        data_type: ty.to_string(),
    };
    let columns = vec![
        col("path", "Utf8"),
        col("name", "Utf8"),
        col("extension", "Utf8"),
        col("size", "Int64"),
        col("modified", "Timestamp(Microsecond, None)"),
        col("etag", "Utf8"),
        col("version", "Utf8"),
    ];

    let opt_str = |v: &Option<String>| match v {
        Some(s) => CellValue::String(s.clone()),
        None => CellValue::Null,
    };
    let rows: Vec<Vec<CellValue>> = entries
        .iter()
        .filter(|e| !e.is_prefix)
        .map(|e| {
            let (path, name) = split_key(&e.key);
            vec![
                CellValue::String(path),
                CellValue::String(name.clone()),
                CellValue::String(extension_of(&name)),
                match e.size {
                    Some(n) => CellValue::Int(n as i64),
                    None => CellValue::Null,
                },
                match &e.modified {
                    Some(m) => CellValue::DateTime(m.format("%Y-%m-%d %H:%M:%S").to_string()),
                    None => CellValue::Null,
                },
                opt_str(&e.etag),
                opt_str(&e.version),
            ]
        })
        .collect();

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.format_name = Some("Cloud inventory".to_string());
    table
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
