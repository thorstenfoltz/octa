//! SAS transport and QVD files written by other programs' libraries
//! (`pyreadstat` for `.xpt`, `pyqvd` for `.qvd`), read back and compared with
//! the CSV they were made from. The readers' own unit tests build their files
//! by hand, so only this proves the layout is the real one.
//!
//! The fixtures were made from `samples/tables/products.csv`:
//! `pyreadstat.write_xport(df, path, file_format_version=5 | 8)` and
//! `QvdTable.from_pandas(df).to_qvd(path)`, with `released` as a date and
//! `in_stock` as 1/0.

use octa::data::DataTable;
use octa::formats::FormatRegistry;
use std::path::{Path, PathBuf};

fn root(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> DataTable {
    let path = root(rel);
    FormatRegistry::new()
        .reader_for_path(&path)
        .unwrap()
        .read_file(&path)
        .unwrap()
}

fn same_as_the_csv(rel: &str) {
    let csv = read("samples/tables/products.csv");
    let got = read(rel);
    let names =
        |t: &DataTable| -> Vec<String> { t.columns.iter().map(|c| c.name.clone()).collect() };
    assert_eq!(names(&got), names(&csv), "{rel}");
    let types: Vec<&str> = got.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(
        types,
        [
            "Int64", "Utf8", "Utf8", "Float64", "Int64", "Date", "Float64", "Utf8"
        ],
        "{rel}"
    );
    assert_eq!(got.rows.len(), csv.rows.len(), "{rel}");
    // in_stock (4) is true/false in the CSV and 1/0 here.
    for (r, (a, b)) in got.rows.iter().zip(&csv.rows).enumerate() {
        for col in [0, 1, 2, 3, 5, 6, 7] {
            assert_eq!(
                a[col].to_string(),
                b[col].to_string(),
                "{rel} row {r} col {col}"
            );
        }
        let stock = if b[4].to_string() == "true" { "1" } else { "0" };
        assert_eq!(a[4].to_string(), stock, "{rel} row {r}");
    }
}

#[test]
fn a_version_5_transport_file_reads_like_its_source() {
    same_as_the_csv("tests/fixtures/products.xpt");
}

#[test]
fn a_version_8_transport_file_reads_like_its_source() {
    same_as_the_csv("tests/fixtures/products_v8.xpt");
}

#[test]
fn a_qvd_file_reads_like_its_source() {
    same_as_the_csv("tests/fixtures/products.qvd");
}

#[test]
fn only_the_transport_file_can_be_written() {
    let registry = FormatRegistry::new();
    for (name, writes) in [("x.xpt", true), ("x.xport", true), ("x.qvd", false)] {
        let reader = registry.reader_for_path(Path::new(name)).unwrap();
        assert_eq!(reader.supports_write(), writes, "{name}");
    }
}

/// What Octa writes reads back as what it was given, in both versions: the
/// sample as it is fits version 5, with one long column name it needs 8.
#[test]
fn a_written_transport_file_reads_back_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let registry = FormatRegistry::new();
    for long_name in [false, true] {
        let mut source = read("tests/fixtures/products.xpt");
        if long_name {
            source.columns[1].name = "product_description".to_string();
        }
        let path = dir.path().join(format!("out{long_name}.xpt"));
        let reader = registry.reader_for_path(&path).unwrap();
        reader.write_file(&path, &source).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() % 80, 0);
        assert_eq!(
            &bytes[20..27],
            if long_name { b"LIBV8  " } else { b"LIBRARY" }
        );
        let back = reader.read_file(&path).unwrap();
        let shape = |t: &DataTable| -> Vec<(String, String)> {
            t.columns
                .iter()
                .map(|c| (c.name.clone(), c.data_type.clone()))
                .collect()
        };
        assert_eq!(shape(&back), shape(&source));
        assert_eq!(back.rows, source.rows);
    }
}
