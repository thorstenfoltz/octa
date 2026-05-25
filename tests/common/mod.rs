#![allow(dead_code)]

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn sample_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "active".into(),
                data_type: "Boolean".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::String("Alice".into()),
                CellValue::Bool(true),
            ],
            vec![
                CellValue::Int(2),
                CellValue::String("Bob".into()),
                CellValue::Bool(false),
            ],
            vec![
                CellValue::Int(3),
                CellValue::String("Charlie".into()),
                CellValue::Bool(true),
            ],
        ],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

/// Generate binary fixture files (parquet, avro, arrow, xlsx) if they don't exist.
pub fn ensure_fixtures() {
    INIT.call_once(|| {
        let registry = FormatRegistry::new();
        let table = sample_table();

        let binary_fixtures: &[(&str, &str)] = &[
            ("sample.parquet", "parquet"),
            ("sample.avro", "avro"),
            ("sample.arrow", "arrow"),
            ("sample.xlsx", "xlsx"),
            ("sample.orc", "orc"),
            ("sample.dbf", "dbf"),
        ];

        for (filename, ext) in binary_fixtures {
            let path = fixture_path(filename);
            if !path.exists() {
                let dummy_path = PathBuf::from(format!("dummy.{}", ext));
                let reader = registry.reader_for_path(&dummy_path).unwrap();
                if reader.supports_write() {
                    reader.write_file(&path, &table).unwrap();
                }
            }
        }

        // NetCDF v3 can't be written via the FormatReader trait (the reader
        // is intentionally read-only) but the `netcdf3` crate exposes a
        // FileWriter. Generate a small fixture once so `tests/netcdf_tests.rs`
        // has something to load.
        let nc_path = fixture_path("sample.nc");
        if !nc_path.exists() {
            write_netcdf3_fixture(&nc_path);
        }
    });
}

/// Build a 5-row NetCDF v3 fixture with two 1D variables (`temperature: f64`,
/// `count: i32`) sharing dimension `time`. Used by `tests/netcdf_tests.rs`.
fn write_netcdf3_fixture(path: &PathBuf) {
    use netcdf3::{DataSet, FileWriter};

    let mut data_set = DataSet::new();
    data_set.add_fixed_dim("time", 5).unwrap();
    data_set
        .add_var_f64::<&str>("temperature", &["time"])
        .unwrap();
    data_set.add_var_i32::<&str>("count", &["time"]).unwrap();

    let mut writer = FileWriter::open(path).unwrap();
    writer
        .set_def(&data_set, netcdf3::Version::Classic, 0)
        .unwrap();
    writer
        .write_var_f64("temperature", &[20.0_f64, 21.5, 22.0, 19.5, 18.0])
        .unwrap();
    writer
        .write_var_i32("count", &[10_i32, 20, 30, 25, 15])
        .unwrap();
    writer.close().unwrap();
}
