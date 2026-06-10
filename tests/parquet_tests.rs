use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatReader;
use octa::formats::parquet_reader::*;
use parquet::arrow::ArrowWriter;
use std::collections::HashMap;
use std::sync::Arc;

// --- data_type_from_string ---

#[test]
fn test_data_type_boolean() {
    assert_eq!(data_type_from_string("boolean"), DataType::Boolean);
    assert_eq!(data_type_from_string("bool"), DataType::Boolean);
    assert_eq!(data_type_from_string("Boolean"), DataType::Boolean);
}

#[test]
fn test_data_type_integers() {
    assert_eq!(data_type_from_string("int8"), DataType::Int8);
    assert_eq!(data_type_from_string("int16"), DataType::Int16);
    assert_eq!(data_type_from_string("int32"), DataType::Int32);
    assert_eq!(data_type_from_string("int64"), DataType::Int64);
    assert_eq!(data_type_from_string("int"), DataType::Int64);
    assert_eq!(data_type_from_string("Int64"), DataType::Int64);
}

#[test]
fn test_data_type_unsigned() {
    assert_eq!(data_type_from_string("uint8"), DataType::UInt8);
    assert_eq!(data_type_from_string("uint16"), DataType::UInt16);
    assert_eq!(data_type_from_string("uint32"), DataType::UInt32);
    assert_eq!(data_type_from_string("uint64"), DataType::UInt64);
}

#[test]
fn test_data_type_floats() {
    assert_eq!(data_type_from_string("float16"), DataType::Float16);
    assert_eq!(data_type_from_string("float32"), DataType::Float32);
    assert_eq!(data_type_from_string("float64"), DataType::Float64);
    assert_eq!(data_type_from_string("float"), DataType::Float64);
    assert_eq!(data_type_from_string("double"), DataType::Float64);
}

#[test]
fn test_data_type_strings() {
    assert_eq!(data_type_from_string("utf8"), DataType::Utf8);
    assert_eq!(data_type_from_string("string"), DataType::Utf8);
    assert_eq!(data_type_from_string("largeutf8"), DataType::LargeUtf8);
    assert_eq!(data_type_from_string("largestring"), DataType::LargeUtf8);
}

#[test]
fn test_data_type_binary() {
    assert_eq!(data_type_from_string("binary"), DataType::Binary);
    assert_eq!(data_type_from_string("largebinary"), DataType::LargeBinary);
}

#[test]
fn test_data_type_dates() {
    assert_eq!(data_type_from_string("date32"), DataType::Date32);
    assert_eq!(data_type_from_string("date"), DataType::Date32);
    assert_eq!(data_type_from_string("date64"), DataType::Date64);
}

#[test]
fn test_data_type_timestamp() {
    assert_eq!(
        data_type_from_string("timestamp(microsecond, none)"),
        DataType::Timestamp(TimeUnit::Microsecond, None)
    );
    assert_eq!(
        data_type_from_string("datetime"),
        DataType::Timestamp(TimeUnit::Microsecond, None)
    );
}

#[test]
fn test_data_type_unknown_fallback() {
    assert_eq!(data_type_from_string("unknown_type"), DataType::Utf8);
}

// --- arrow_value_to_cell ---

#[test]
fn test_arrow_boolean() {
    let arr = BooleanArray::from(vec![Some(true), Some(false), None]);
    assert_eq!(arrow_value_to_cell(&arr, 0), CellValue::Bool(true));
    assert_eq!(arrow_value_to_cell(&arr, 1), CellValue::Bool(false));
    assert_eq!(arrow_value_to_cell(&arr, 2), CellValue::Null);
}

#[test]
fn test_arrow_int64() {
    let arr = Int64Array::from(vec![Some(42), Some(-1), None]);
    assert_eq!(arrow_value_to_cell(&arr, 0), CellValue::Int(42));
    assert_eq!(arrow_value_to_cell(&arr, 1), CellValue::Int(-1));
    assert_eq!(arrow_value_to_cell(&arr, 2), CellValue::Null);
}

#[test]
fn test_arrow_float64() {
    let arr = Float64Array::from(vec![Some(2.5), None]);
    assert_eq!(arrow_value_to_cell(&arr, 0), CellValue::Float(2.5));
    assert_eq!(arrow_value_to_cell(&arr, 1), CellValue::Null);
}

#[test]
fn test_arrow_string() {
    let arr = StringArray::from(vec![Some("hello"), None]);
    assert_eq!(
        arrow_value_to_cell(&arr, 0),
        CellValue::String("hello".into())
    );
    assert_eq!(arrow_value_to_cell(&arr, 1), CellValue::Null);
}

#[test]
fn test_arrow_date32() {
    let arr = Date32Array::from(vec![Some(19737)]);
    match arrow_value_to_cell(&arr, 0) {
        CellValue::Date(s) => assert_eq!(s, "2024-01-15"),
        other => panic!("Expected Date, got {:?}", other),
    }
}

#[test]
fn test_arrow_binary() {
    let arr = BinaryArray::from(vec![Some(b"abc".as_ref())]);
    assert_eq!(
        arrow_value_to_cell(&arr, 0),
        CellValue::Binary(vec![97, 98, 99])
    );
}

#[test]
fn test_arrow_int_types() {
    let i8_arr = Int8Array::from(vec![Some(10i8)]);
    assert_eq!(arrow_value_to_cell(&i8_arr, 0), CellValue::Int(10));

    let i16_arr = Int16Array::from(vec![Some(1000i16)]);
    assert_eq!(arrow_value_to_cell(&i16_arr, 0), CellValue::Int(1000));

    let i32_arr = Int32Array::from(vec![Some(100000i32)]);
    assert_eq!(arrow_value_to_cell(&i32_arr, 0), CellValue::Int(100000));

    let u8_arr = UInt8Array::from(vec![Some(255u8)]);
    assert_eq!(arrow_value_to_cell(&u8_arr, 0), CellValue::Int(255));

    let u32_arr = UInt32Array::from(vec![Some(4_000_000u32)]);
    assert_eq!(arrow_value_to_cell(&u32_arr, 0), CellValue::Int(4_000_000));
}

// --- parquet round-trip ---

#[test]
fn test_parquet_round_trip() {
    let table = DataTable {
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
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::String("Alice".into()),
                CellValue::Float(9.5),
            ],
            vec![
                CellValue::Int(2),
                CellValue::String("Bob".into()),
                CellValue::Float(7.0),
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
    };

    let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    ParquetReader.write_file(f.path(), &table).unwrap();

    let table2 = ParquetReader.read_file(f.path()).unwrap();
    assert_eq!(table2.row_count(), 2);
    assert_eq!(table2.col_count(), 3);
    assert_eq!(table2.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(table2.get(0, 1), Some(&CellValue::String("Alice".into())));
    assert_eq!(table2.get(1, 2), Some(&CellValue::Float(7.0)));
}

/// Write a parquet file shaped like a pandas-emitted DataFrame: a
/// regular data column plus an `__index_level_0__` column carrying the
/// row index, with the `pandas` JSON metadata listing the index column.
/// The reader must drop the index column from both the schema and the
/// per-row data so the user sees only the real data column(s).
#[test]
fn test_parquet_strips_pandas_index_column() {
    let id = Field::new("id", DataType::Int64, false);
    let val = Field::new("value", DataType::Utf8, false);
    let idx = Field::new("__index_level_0__", DataType::Int64, false);

    // Minimal pandas-style metadata: only `index_columns` matters here.
    let pandas_meta = r#"{"index_columns": ["__index_level_0__"], "columns": []}"#;
    let mut schema_meta = HashMap::new();
    schema_meta.insert("pandas".to_string(), pandas_meta.to_string());

    let schema = Arc::new(Schema::new_with_metadata(vec![id, val, idx], schema_meta));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![10, 20, 30])) as Arc<dyn Array>,
            Arc::new(StringArray::from(vec!["a", "b", "c"])) as Arc<dyn Array>,
            // Pandas writes a 0..N RangeIndex as a real Int64 column.
            Arc::new(Int64Array::from(vec![0i64, 1, 2])) as Arc<dyn Array>,
        ],
    )
    .unwrap();

    let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    let file = std::fs::File::create(f.path()).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let read = ParquetReader.read_file(f.path()).unwrap();
    assert_eq!(read.col_count(), 2, "index column should be stripped");
    let names: Vec<&str> = read.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "value"]);
    assert_eq!(read.row_count(), 3);
    assert_eq!(read.get(0, 0), Some(&CellValue::Int(10)));
    assert_eq!(read.get(2, 1), Some(&CellValue::String("c".into())));
}

/// A file with a bare `__index_level_0__` column but no `pandas`
/// metadata block (older pandas releases dropped the metadata) must
/// still have the column stripped — the default-name list catches it.
#[test]
fn test_parquet_strips_default_pandas_index_name_without_metadata() {
    let id = Field::new("id", DataType::Int64, false);
    let idx = Field::new("__index_level_0__", DataType::Int64, false);

    let schema = Arc::new(Schema::new(vec![id, idx]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])) as Arc<dyn Array>,
            Arc::new(Int64Array::from(vec![0i64, 1])) as Arc<dyn Array>,
        ],
    )
    .unwrap();

    let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    let file = std::fs::File::create(f.path()).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    let read = ParquetReader.read_file(f.path()).unwrap();
    assert_eq!(read.col_count(), 1);
    assert_eq!(read.columns[0].name, "id");
    assert_eq!(read.row_count(), 2);
    assert_eq!(read.get(1, 0), Some(&CellValue::Int(2)));
}

// --- every-declared-type round-trip ---

/// One-column DataTable with a value row and a null row.
fn one_col_table(type_name: &str, value: CellValue) -> DataTable {
    DataTable {
        columns: vec![ColumnInfo {
            name: "col".into(),
            data_type: type_name.into(),
        }],
        rows: vec![vec![value], vec![CellValue::Null]],
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

/// Every type name `data_type_from_string` maps must write without error and
/// read back with the value intact (narrow ints widen to `CellValue::Int` on
/// read; that is the documented expectation). Before the width-correct
/// builder arms in `build_arrow_array`, most of these failed at write time
/// with a schema/array mismatch from `RecordBatch::try_new`.
#[test]
fn test_parquet_round_trip_every_declared_type() {
    let cases: Vec<(&str, CellValue, CellValue)> = vec![
        ("boolean", CellValue::Bool(true), CellValue::Bool(true)),
        ("int8", CellValue::Int(7), CellValue::Int(7)),
        ("int16", CellValue::Int(-300), CellValue::Int(-300)),
        ("int32", CellValue::Int(100_000), CellValue::Int(100_000)),
        ("int64", CellValue::Int(1), CellValue::Int(1)),
        ("uint8", CellValue::Int(255), CellValue::Int(255)),
        ("uint16", CellValue::Int(65_535), CellValue::Int(65_535)),
        (
            "uint32",
            CellValue::Int(4_000_000),
            CellValue::Int(4_000_000),
        ),
        ("uint64", CellValue::Int(42), CellValue::Int(42)),
        // 1.5 / 2.5 are exactly representable in f16/f32.
        ("float16", CellValue::Float(1.5), CellValue::Float(1.5)),
        ("float32", CellValue::Float(2.5), CellValue::Float(2.5)),
        ("float64", CellValue::Float(3.25), CellValue::Float(3.25)),
        (
            "utf8",
            CellValue::String("hi".into()),
            CellValue::String("hi".into()),
        ),
        (
            "largeutf8",
            CellValue::String("big".into()),
            CellValue::String("big".into()),
        ),
        (
            "binary",
            CellValue::Binary(vec![1, 2, 3]),
            CellValue::Binary(vec![1, 2, 3]),
        ),
        (
            "largebinary",
            CellValue::Binary(vec![4, 5]),
            CellValue::Binary(vec![4, 5]),
        ),
        (
            "date32",
            CellValue::Date("2024-01-15".into()),
            CellValue::Date("2024-01-15".into()),
        ),
        (
            "date64",
            CellValue::Date("2024-01-15".into()),
            CellValue::Date("2024-01-15".into()),
        ),
        (
            "datetime",
            CellValue::DateTime("2024-01-02 03:04:05".into()),
            CellValue::DateTime("2024-01-02 03:04:05.000".into()),
        ),
    ];

    for (type_name, write_value, expect) in cases {
        let table = one_col_table(type_name, write_value);
        let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
        ParquetReader
            .write_file(f.path(), &table)
            .unwrap_or_else(|e| panic!("writing a {type_name} column failed: {e}"));
        let read = ParquetReader
            .read_file(f.path())
            .unwrap_or_else(|e| panic!("reading a {type_name} column back failed: {e}"));
        assert_eq!(read.row_count(), 2, "{type_name}: row count");
        assert_eq!(
            read.get(0, 0),
            Some(&expect),
            "{type_name}: value did not survive the round-trip"
        );
        assert_eq!(
            read.get(1, 0),
            Some(&CellValue::Null),
            "{type_name}: null did not survive the round-trip"
        );
    }
}

/// Values that do not fit the declared width become null instead of failing
/// the write or wrapping silently.
#[test]
fn test_parquet_out_of_range_narrow_int_becomes_null() {
    let table = one_col_table("int8", CellValue::Int(300));
    let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    ParquetReader.write_file(f.path(), &table).unwrap();
    let read = ParquetReader.read_file(f.path()).unwrap();
    assert_eq!(read.get(0, 0), Some(&CellValue::Null));

    let table = one_col_table("uint8", CellValue::Int(-1));
    let f = tempfile::NamedTempFile::with_suffix(".parquet").unwrap();
    ParquetReader.write_file(f.path(), &table).unwrap();
    let read = ParquetReader.read_file(f.path()).unwrap();
    assert_eq!(read.get(0, 0), Some(&CellValue::Null));
}

/// The Arrow IPC writer shares `data_type_from_string` and
/// `build_arrow_array`, so a multi-typed table must round-trip there too.
#[test]
fn test_arrow_ipc_round_trip_narrow_types() {
    use octa::formats::arrow_ipc_reader::ArrowIpcReader;
    let table = DataTable {
        columns: vec![
            ColumnInfo {
                name: "small".into(),
                data_type: "Int32".into(),
            },
            ColumnInfo {
                name: "ratio".into(),
                data_type: "Float32".into(),
            },
            ColumnInfo {
                name: "blob".into(),
                data_type: "Binary".into(),
            },
        ],
        rows: vec![vec![
            CellValue::Int(123),
            CellValue::Float(0.5),
            CellValue::Binary(vec![9, 8, 7]),
        ]],
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
    };
    let f = tempfile::NamedTempFile::with_suffix(".arrow").unwrap();
    ArrowIpcReader.write_file(f.path(), &table).unwrap();
    let read = ArrowIpcReader.read_file(f.path()).unwrap();
    assert_eq!(read.get(0, 0), Some(&CellValue::Int(123)));
    assert_eq!(read.get(0, 1), Some(&CellValue::Float(0.5)));
    assert_eq!(read.get(0, 2), Some(&CellValue::Binary(vec![9, 8, 7])));
}
