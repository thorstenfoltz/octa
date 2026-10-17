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
