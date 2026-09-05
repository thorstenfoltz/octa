//! ORC writing: the type map has to stay inside what `orc-rust`'s encoder can
//! actually encode.

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;

fn table_with(data_type: &str, cell: CellValue) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "value".to_string(),
            data_type: data_type.to_string(),
        },
    ];
    t.rows = vec![vec![CellValue::Int(1), cell]];
    t
}

/// `orc-rust`'s `create_encoder` covers signed ints, floats, bool, Utf8 and
/// Binary and `unimplemented!()`s everything else, so naming a type it does
/// not know **panics the process**. Writing a table with an inferred date
/// column used to abort `octa --convert` with a Rust backtrace; every one of
/// these must come back as an ordinary write instead.
#[test]
fn types_orc_cannot_encode_are_written_as_text_not_a_panic() {
    let registry = FormatRegistry::new();
    let reader = registry.reader_by_name("ORC").expect("ORC reader");
    let cases = [
        ("Date32", CellValue::Date("2024-01-15".to_string())),
        ("Date64", CellValue::Date("2024-01-15".to_string())),
        (
            "DateTime",
            CellValue::DateTime("2024-01-15 08:30:00".to_string()),
        ),
        ("UInt64", CellValue::Int(9_000_000_000)),
    ];
    for (data_type, cell) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.orc");
        reader
            .write_file(&path, &table_with(data_type, cell.clone()))
            .unwrap_or_else(|e| panic!("writing a '{data_type}' column: {e:#}"));

        let back = reader.read_file(&path).unwrap();
        assert_eq!(back.row_count(), 1, "'{data_type}' lost its row");
        assert_eq!(
            back.get(0, 1).map(ToString::to_string),
            Some(cell.to_string()),
            "'{data_type}' did not survive the round trip"
        );
    }
}

/// The types ORC does encode natively must keep doing so, and the narrow ints
/// must widen into one it can rather than into one `build_record_batch` cannot
/// build - `Int8` and `Int16` named a schema type whose array was never built,
/// so they failed the write before this ever reached the encoder.
#[test]
fn types_orc_encodes_natively_keep_their_arrow_type() {
    let registry = FormatRegistry::new();
    let reader = registry.reader_by_name("ORC").expect("ORC reader");
    for (data_type, cell, expect) in [
        ("Int8", CellValue::Int(7), "Int32"),
        ("Int16", CellValue::Int(300), "Int32"),
        ("Int32", CellValue::Int(70_000), "Int32"),
        ("Int64", CellValue::Int(42), "Int64"),
        ("UInt16", CellValue::Int(60_000), "Int32"),
        ("UInt32", CellValue::Int(4_000_000_000), "Int64"),
        ("Float64", CellValue::Float(1.5), "Float64"),
        ("Boolean", CellValue::Bool(true), "Boolean"),
        ("Utf8", CellValue::String("hi".to_string()), "Utf8"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.orc");
        reader
            .write_file(&path, &table_with(data_type, cell))
            .unwrap();
        let back = reader.read_file(&path).unwrap();
        assert_eq!(back.columns[1].data_type, expect, "for {data_type}");
    }
}
