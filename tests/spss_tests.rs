//! Tests for the SPSS (.sav) reader. Generates the test fixture programmatically
//! using `ambers::write_sav` so we don't ship binary blobs.

use std::sync::Arc;
use tempfile::TempDir;

use ambers::{Compression, metadata::SpssMetadata};
use arrow57::array::{Float64Array, RecordBatch, StringArray};
use arrow57::datatypes::{DataType, Field, Schema};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;

fn make_sav(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("sample.sav");

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Float64, true),
        Field::new("name", DataType::Utf8, true),
        Field::new("score", DataType::Float64, true),
    ]));

    let id = Float64Array::from(vec![Some(1.0), Some(2.0), Some(3.0), None]);
    let name = StringArray::from(vec![
        Some("Alice"),
        Some("Bob"),
        Some("Charlie"),
        Some("Dana"),
    ]);
    let score = Float64Array::from(vec![Some(90.5), Some(82.0), None, Some(77.3)]);

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(id), Arc::new(name), Arc::new(score)],
    )
    .expect("build SPSS RecordBatch");

    let metadata = SpssMetadata::from_arrow_schema(&schema);
    ambers::write_sav(&path, &batch, &metadata, Compression::None, None).expect("write .sav");

    path
}

#[test]
fn spss_reader_reads_basic_file() {
    let dir = TempDir::new().unwrap();
    let path = make_sav(&dir);

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("reader for .sav");
    assert_eq!(reader.name(), "SPSS");

    let table = reader.read_file(&path).expect("read .sav");
    assert_eq!(table.format_name.as_deref(), Some("SPSS"));
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.row_count(), 4);

    let cols: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["id", "name", "score"]);

    // Row 0: (1.0, "Alice", 90.5)
    assert!(matches!(
        table.get(0, 0),
        Some(CellValue::Float(v)) if (*v - 1.0).abs() < 1e-9
    ));
    assert!(matches!(
        table.get(0, 1),
        Some(CellValue::String(s)) if s == "Alice"
    ));

    // Row 2: score is null
    assert!(matches!(table.get(2, 2), Some(CellValue::Null)));

    // Row 3: id is null, name "Dana"
    assert!(matches!(table.get(3, 0), Some(CellValue::Null)));
    assert!(matches!(
        table.get(3, 1),
        Some(CellValue::String(s)) if s == "Dana"
    ));
}

#[test]
fn spss_reader_supports_zsav_extension() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("foo.zsav");
    let reader = registry.reader_for_path(dummy).expect("reader for .zsav");
    assert_eq!(reader.name(), "SPSS");
}

#[test]
fn spss_reader_supports_write() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("foo.sav");
    let reader = registry.reader_for_path(dummy).unwrap();
    assert!(reader.supports_write());
}

fn make_simple_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "name".to_string(),
            data_type: "Utf8".to_string(),
        },
        ColumnInfo {
            name: "score".to_string(),
            data_type: "Float64".to_string(),
        },
        ColumnInfo {
            name: "active".to_string(),
            data_type: "Boolean".to_string(),
        },
        ColumnInfo {
            name: "born".to_string(),
            data_type: "Date".to_string(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("Alice".to_string()),
            CellValue::Float(90.5),
            CellValue::Bool(true),
            CellValue::Date("1990-01-15".to_string()),
        ],
        vec![
            CellValue::Int(2),
            CellValue::String("Bob".to_string()),
            CellValue::Null,
            CellValue::Bool(false),
            CellValue::Date("1985-06-30".to_string()),
        ],
        vec![
            CellValue::Null,
            CellValue::String("Charlie".to_string()),
            CellValue::Float(77.3),
            CellValue::Null,
            CellValue::Null,
        ],
    ];
    t
}

#[test]
fn spss_writer_round_trips_basic_types() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("out.sav");

    let table = make_simple_table();
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("reader for .sav");
    reader.write_file(&path, &table).expect("write .sav");

    let read = reader.read_file(&path).expect("read back .sav");
    assert_eq!(read.col_count(), 5);
    assert_eq!(read.row_count(), 3);

    let cols: Vec<&str> = read.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["id", "name", "score", "active", "born"]);

    // SPSS stores all numerics as Float64 internally — Int64/Boolean
    // both come back as Float64 columns.
    assert_eq!(read.columns[0].data_type, "Float64");
    assert_eq!(read.columns[1].data_type, "Utf8");
    assert_eq!(read.columns[2].data_type, "Float64");
    assert_eq!(read.columns[3].data_type, "Float64");
    assert_eq!(read.columns[4].data_type, "Date");

    // Row 0
    assert!(matches!(
        read.get(0, 0),
        Some(CellValue::Float(v)) if (*v - 1.0).abs() < 1e-9
    ));
    assert!(matches!(read.get(0, 1), Some(CellValue::String(s)) if s == "Alice"));
    assert!(matches!(
        read.get(0, 2),
        Some(CellValue::Float(v)) if (*v - 90.5).abs() < 1e-9
    ));
    assert!(matches!(
        read.get(0, 3),
        Some(CellValue::Float(v)) if (*v - 1.0).abs() < 1e-9
    ));
    assert!(matches!(read.get(0, 4), Some(CellValue::Date(s)) if s == "1990-01-15"));

    // Row 1: score is Null
    assert!(matches!(read.get(1, 2), Some(CellValue::Null)));

    // Row 2: id and active are Null
    assert!(matches!(read.get(2, 0), Some(CellValue::Null)));
    assert!(matches!(read.get(2, 3), Some(CellValue::Null)));
    assert!(matches!(read.get(2, 4), Some(CellValue::Null)));
}

#[test]
fn spss_writer_round_trips_zsav_extension() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("out.zsav");

    let table = make_simple_table();
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("reader for .zsav");
    reader.write_file(&path, &table).expect("write .zsav");

    let read = reader.read_file(&path).expect("read back .zsav");
    assert_eq!(read.col_count(), 5);
    assert_eq!(read.row_count(), 3);
    assert!(matches!(read.get(0, 1), Some(CellValue::String(s)) if s == "Alice"));
}

#[test]
fn spss_writer_applies_pending_edits_before_writing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("edited.sav");

    let mut table = make_simple_table();
    table.set(0, 1, CellValue::String("Alicia".to_string()));

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader.write_file(&path, &table).expect("write .sav");

    let read = reader.read_file(&path).unwrap();
    assert!(matches!(read.get(0, 1), Some(CellValue::String(s)) if s == "Alicia"));
}
