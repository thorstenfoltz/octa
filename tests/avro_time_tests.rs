use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatReader;
use octa::formats::avro_reader::AvroReader;

#[test]
fn avro_time_roundtrips_and_duration_reads_as_text() {
    let t = DataTable {
        columns: vec![
            ColumnInfo {
                name: "at".into(),
                data_type: "Time64(Microsecond)".into(),
            },
            ColumnInfo {
                name: "worked".into(),
                data_type: "Duration(Microsecond)".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Time(9 * 3_600_000_000_000 + 30 * 60_000_000_000),
                CellValue::Duration(30 * 3_600_000_000_000 + 15 * 60_000_000_000),
            ],
            vec![CellValue::Null, CellValue::Null],
        ],
        ..DataTable::empty()
    };
    let tmp = tempfile::NamedTempFile::with_suffix(".avro").unwrap();
    AvroReader.write_file(tmp.path(), &t).unwrap();
    let back = AvroReader.read_file(tmp.path()).unwrap();
    assert_eq!(back.columns[0].data_type, "Time64(µs)");
    assert_eq!(back.get(0, 0), Some(&CellValue::Time(34_200_000_000_000)));
    assert_eq!(back.get(1, 0), Some(&CellValue::Null));
    assert_eq!(back.get(0, 1), Some(&CellValue::String("30:15:00".into())));
}

#[test]
fn avro_duration_logical_type_reads_typed() {
    use apache_avro::{Days, Millis, Months, Schema, Writer, types::Value};
    let schema = Schema::parse_str(
        r#"{"type":"record","name":"R","fields":[{"name":"d","type":{"type":"fixed","name":"dur","size":12,"logicalType":"duration"}}]}"#,
    )
    .unwrap();
    let mut w = Writer::new(&schema, Vec::new()).unwrap();
    let d = apache_avro::Duration::new(Months::new(0), Days::new(1), Millis::new(3_600_000));
    w.append_value(Value::Record(vec![("d".into(), Value::Duration(d))]))
        .unwrap();
    let bytes = w.into_inner().unwrap();
    let tmp = tempfile::NamedTempFile::with_suffix(".avro").unwrap();
    std::fs::write(tmp.path(), bytes).unwrap();
    let back = AvroReader.read_file(tmp.path()).unwrap();
    assert!(back.columns[0].data_type.starts_with("Duration"));
    assert_eq!(
        back.get(0, 0),
        Some(&CellValue::Duration(25 * 3_600_000_000_000))
    );
}
