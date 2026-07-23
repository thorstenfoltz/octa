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
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(key: &str, size: u64) -> ObjectEntry {
        ObjectEntry {
            name: key.rsplit('/').next().unwrap_or("").to_string(),
            key: key.to_string(),
            is_prefix: false,
            size: Some(size),
            modified: Some(chrono::Utc.with_ymd_and_hms(2026, 7, 13, 8, 0, 0).unwrap()),
            etag: Some("\"abc\"".to_string()),
            version: None,
        }
    }

    #[test]
    fn columns_are_the_documented_seven() {
        let t = build_inventory_table(&[entry("a.csv", 10)]);
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "path",
                "name",
                "extension",
                "size",
                "modified",
                "etag",
                "version"
            ]
        );
        assert_eq!(t.columns[3].data_type, "Int64");
        assert_eq!(t.columns[4].data_type, "Timestamp(Microsecond, None)");
    }

    #[test]
    fn splits_path_before_name_like_find() {
        let t = build_inventory_table(&[entry("sub/deep/c.csv", 5), entry("a.csv", 1)]);
        assert_eq!(t.rows[0][0], CellValue::String("sub/deep".into()));
        assert_eq!(t.rows[0][1], CellValue::String("c.csv".into()));
        assert_eq!(t.rows[0][2], CellValue::String("csv".into()));
        assert_eq!(t.rows[1][0], CellValue::String(String::new()));
    }

    #[test]
    fn sizes_are_ints_and_missing_metadata_is_null() {
        let mut e = entry("x.parquet", 42);
        e.etag = None;
        let t = build_inventory_table(&[e]);
        assert_eq!(t.rows[0][3], CellValue::Int(42));
        assert_eq!(t.rows[0][5], CellValue::Null);
        assert_eq!(t.rows[0][6], CellValue::Null);
    }

    #[test]
    fn folder_entries_are_skipped() {
        let mut folder = entry("sub/", 0);
        folder.is_prefix = true;
        folder.size = None;
        let t = build_inventory_table(&[folder, entry("sub/f.csv", 1)]);
        assert_eq!(t.row_count(), 1);
    }
}
