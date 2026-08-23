//! Multi-sheet `.xlsx` writing: one workbook, one worksheet per table.
//!
//! The single-table writer is a one-element call into `write_workbook`, so
//! these tests also cover the ordinary Save path staying intact.

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::{FormatRegistry, excel_reader};

/// Sheet names in workbook order.
fn sheet_names(reader: &dyn octa::formats::FormatReader, path: &std::path::Path) -> Vec<String> {
    reader
        .list_tables(path)
        .expect("list sheets")
        .expect("xlsx is a multi-table source")
        .into_iter()
        .map(|t| t.name)
        .collect()
}

fn table(col: &str, values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: col.to_string(),
        data_type: "Utf8".into(),
    }];
    t.rows = values
        .iter()
        .map(|v| vec![CellValue::String((*v).to_string())])
        .collect();
    t
}

#[test]
fn workbook_writes_one_sheet_per_table_and_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.xlsx");
    let a = table("alpha", &["x", "y"]);
    let b = table("beta", &["z"]);

    excel_reader::write_workbook(
        &path,
        &[("First".into(), &a, None), ("Second".into(), &b, None)],
    )
    .expect("write workbook");

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("xlsx reader");
    let sheets = sheet_names(reader, &path);
    assert_eq!(sheets, vec!["First".to_string(), "Second".to_string()]);

    let first = reader.read_table(&path, "First").expect("read First");
    assert_eq!(first.columns[0].name, "alpha");
    assert_eq!(first.row_count(), 2);

    let second = reader.read_table(&path, "Second").expect("read Second");
    assert_eq!(second.columns[0].name, "beta");
    assert_eq!(second.row_count(), 1);
}

#[test]
fn colliding_and_illegal_sheet_names_are_corrected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("names.xlsx");
    let t = table("v", &["1"]);

    // Two tabs may legitimately share a name, and a file name can carry
    // characters Excel refuses in a sheet name.
    excel_reader::write_workbook(
        &path,
        &[
            ("Report".into(), &t, None),
            ("Report".into(), &t, None),
            ("2024/Q1".into(), &t, None),
        ],
    )
    .expect("write workbook");

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("xlsx reader");
    let sheets = sheet_names(reader, &path);
    assert_eq!(
        sheets,
        vec![
            "Report".to_string(),
            "Report_2".to_string(),
            "2024_Q1".to_string()
        ]
    );
}

#[test]
fn an_empty_workbook_is_refused_rather_than_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.xlsx");
    assert!(excel_reader::write_workbook(&path, &[]).is_err());
    assert!(!path.exists(), "nothing should be written");
}

#[test]
fn the_single_table_save_path_still_works() {
    // write_file goes through write_workbook now; a plain save must be
    // unchanged, including the default sheet name.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.xlsx");
    let t = table("only", &["a", "b", "c"]);

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("xlsx reader");
    reader.write_file(&path, &t).expect("write single");

    let sheets = sheet_names(reader, &path);
    assert_eq!(sheets, vec!["Sheet1".to_string()]);
    let back = reader.read_table(&path, "Sheet1").expect("read back");
    assert_eq!(back.row_count(), 3);
    assert_eq!(back.columns[0].name, "only");
}
