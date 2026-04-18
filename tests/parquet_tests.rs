use arrow::array::*;
use arrow::datatypes::{DataType, TimeUnit};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatReader;
use octa::formats::parquet_reader::*;
use std::collections::HashMap;

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
    let arr = Float64Array::from(vec![Some(3.14), None]);
    assert_eq!(arrow_value_to_cell(&arr, 0), CellValue::Float(3.14));
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
