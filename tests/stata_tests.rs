//! Tests for the Stata (.dta) reader. Generates the test fixture programmatically
//! using `dta::DtaWriter`.

use tempfile::TempDir;

use dta::stata::dta::byte_order::ByteOrder;
use dta::stata::dta::dta_writer::DtaWriter;
use dta::stata::dta::header::Header;
use dta::stata::dta::release::Release;
use dta::stata::dta::schema::Schema;
use dta::stata::dta::value::Value;
use dta::stata::dta::variable::Variable;
use dta::stata::dta::variable_type::VariableType;
use dta::stata::missing_value::MissingValue;
use dta::stata::stata_double::StataDouble;
use dta::stata::stata_long::StataLong;
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;

fn make_dta(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("sample.dta");

    let header = Header::builder(Release::V118, ByteOrder::LittleEndian).build();
    let schema = Schema::builder()
        .add_variable(Variable::builder(VariableType::Long, "id").format("%12.0g"))
        .add_variable(Variable::builder(VariableType::FixedString(16), "name").format("%-16s"))
        .add_variable(Variable::builder(VariableType::Double, "score").format("%9.2f"))
        .build()
        .expect("schema");

    let mut record_writer = DtaWriter::new()
        .from_path(&path)
        .expect("open dta")
        .write_header(header)
        .expect("write header")
        .write_schema(schema)
        .expect("write schema")
        .into_record_writer()
        .expect("record writer");

    record_writer
        .write_record(&[
            Value::Long(StataLong::Present(1)),
            Value::string("Alice"),
            Value::Double(StataDouble::Present(90.5)),
        ])
        .expect("row 1");
    record_writer
        .write_record(&[
            Value::Long(StataLong::Present(2)),
            Value::string("Bob"),
            Value::Double(StataDouble::Present(82.0)),
        ])
        .expect("row 2");
    record_writer
        .write_record(&[
            Value::Long(StataLong::Present(3)),
            Value::string("Charlie"),
            Value::Double(StataDouble::Missing(MissingValue::System)),
        ])
        .expect("row 3");

    record_writer
        .into_long_string_writer()
        .expect("long string writer")
        .into_value_label_writer()
        .expect("value label writer")
        .finish()
        .expect("finish");

    path
}

#[test]
fn stata_reader_reads_basic_file() {
    let dir = TempDir::new().unwrap();
    let path = make_dta(&dir);

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("reader for .dta");
    assert_eq!(reader.name(), "Stata");

    let table = reader.read_file(&path).expect("read .dta");
    assert_eq!(table.format_name.as_deref(), Some("Stata"));
    assert_eq!(table.col_count(), 3);
    assert_eq!(table.row_count(), 3);

    let cols: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["id", "name", "score"]);

    // Types: Long → Int32, FixedString → Utf8, Double → Float64
    assert_eq!(table.columns[0].data_type, "Int32");
    assert_eq!(table.columns[1].data_type, "Utf8");
    assert_eq!(table.columns[2].data_type, "Float64");

    // Row 0: (1, "Alice", 90.5)
    assert!(matches!(table.get(0, 0), Some(CellValue::Int(1))));
    assert!(matches!(
        table.get(0, 1),
        Some(CellValue::String(s)) if s == "Alice"
    ));
    assert!(matches!(
        table.get(0, 2),
        Some(CellValue::Float(v)) if (*v - 90.5).abs() < 1e-9
    ));

    // Row 2: score is missing → CellValue::Null
    assert!(matches!(table.get(2, 2), Some(CellValue::Null)));
}

#[test]
fn stata_reader_supports_write() {
    let registry = FormatRegistry::new();
    let dummy = std::path::Path::new("foo.dta");
    let reader = registry.reader_for_path(dummy).unwrap();
    assert!(reader.supports_write());
}

fn make_simple_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int32".to_string(),
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
            name: "big".to_string(),
            data_type: "Int64".to_string(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("Alice".to_string()),
            CellValue::Float(90.5),
            CellValue::Bool(true),
            CellValue::Int(10_000_000_000),
        ],
        vec![
            CellValue::Int(2),
            CellValue::String("Bob".to_string()),
            CellValue::Null,
            CellValue::Bool(false),
            CellValue::Int(20_000_000_000),
        ],
        vec![
            CellValue::Null,
            CellValue::String("Charlie with a longer name".to_string()),
            CellValue::Float(77.3),
            CellValue::Null,
            CellValue::Null,
        ],
    ];
    t
}

#[test]
fn stata_writer_round_trips_basic_types() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("out.dta");

    let table = make_simple_table();
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).expect("reader for .dta");
    reader.write_file(&path, &table).expect("write .dta");

    let read = reader.read_file(&path).expect("read back .dta");
    assert_eq!(read.col_count(), 5);
    assert_eq!(read.row_count(), 3);

    let cols: Vec<&str> = read.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cols, vec!["id", "name", "score", "active", "big"]);

    // Int32 → Long → Int32; Boolean → Byte → Int8; Int64 → Double → Float64.
    assert_eq!(read.columns[0].data_type, "Int32");
    assert_eq!(read.columns[1].data_type, "Utf8");
    assert_eq!(read.columns[2].data_type, "Float64");
    assert_eq!(read.columns[3].data_type, "Int8");
    assert_eq!(read.columns[4].data_type, "Float64");

    // Row 0
    assert!(matches!(read.get(0, 0), Some(CellValue::Int(1))));
    assert!(matches!(read.get(0, 1), Some(CellValue::String(s)) if s == "Alice"));
    assert!(matches!(
        read.get(0, 2),
        Some(CellValue::Float(v)) if (*v - 90.5).abs() < 1e-9
    ));
    assert!(matches!(read.get(0, 3), Some(CellValue::Int(1))));
    assert!(matches!(
        read.get(0, 4),
        Some(CellValue::Float(v)) if (*v - 10_000_000_000.0).abs() < 1.0
    ));

    // Row 1: score is Null
    assert!(matches!(read.get(1, 2), Some(CellValue::Null)));

    // Row 2: id, active, big are Null
    assert!(matches!(read.get(2, 0), Some(CellValue::Null)));
    assert!(matches!(read.get(2, 3), Some(CellValue::Null)));
    assert!(matches!(read.get(2, 4), Some(CellValue::Null)));

    // String column held the wider value
    assert!(matches!(
        read.get(2, 1),
        Some(CellValue::String(s)) if s == "Charlie with a longer name"
    ));
}

#[test]
fn stata_writer_handles_empty_string_column() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty_strings.dta");

    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "tag".to_string(),
        data_type: "Utf8".to_string(),
    }];
    t.rows = vec![
        vec![CellValue::Null],
        vec![CellValue::String(String::new())],
        vec![CellValue::Null],
    ];

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader.write_file(&path, &t).expect("write .dta");
    let read = reader.read_file(&path).expect("read .dta");
    assert_eq!(read.row_count(), 3);
    // FixedString columns where every cell is empty become FixedString(1)
    // by clamping; on read the cells decode to empty strings.
    for r in 0..3 {
        match read.get(r, 0) {
            Some(CellValue::String(s)) => assert!(s.is_empty()),
            other => panic!("row {r}: expected empty string, got {other:?}"),
        }
    }
}

#[test]
fn stata_writer_applies_pending_edits_before_writing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("edited.dta");

    let mut table = make_simple_table();
    table.set(0, 1, CellValue::String("Alicia".to_string()));

    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    reader.write_file(&path, &table).expect("write .dta");

    let read = reader.read_file(&path).unwrap();
    assert!(matches!(read.get(0, 1), Some(CellValue::String(s)) if s == "Alicia"));
}
