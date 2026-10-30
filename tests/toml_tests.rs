use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::toml_reader::*;
use serde_json::json;

#[test]
fn test_toml_string() {
    let v = toml::Value::String("hello".into());
    assert_eq!(toml_to_json(&v), json!("hello"));
}

#[test]
fn test_toml_integer() {
    let v = toml::Value::Integer(42);
    assert_eq!(toml_to_json(&v), json!(42));
}

#[test]
fn test_toml_float() {
    let v = toml::Value::Float(2.5);
    assert_eq!(toml_to_json(&v), json!(2.5));
}

#[test]
fn test_toml_boolean() {
    let v = toml::Value::Boolean(true);
    assert_eq!(toml_to_json(&v), json!(true));
}

#[test]
fn test_toml_array() {
    let v: toml::Value = toml::from_str("arr = [1, 2, 3]").unwrap();
    let j = toml_to_json(&v);
    assert_eq!(j["arr"], json!([1, 2, 3]));
}

#[test]
fn test_toml_table() {
    let v: toml::Value = toml::from_str("[server]\nhost = \"localhost\"\nport = 8080").unwrap();
    let j = toml_to_json(&v);
    assert_eq!(j["server"]["host"], json!("localhost"));
    assert_eq!(j["server"]["port"], json!(8080));
}

#[test]
fn test_toml_read_file() {
    let mut f = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    std::io::Write::write_all(
        &mut f,
        b"[[items]]\nname = \"a\"\nval = 1\n\n[[items]]\nname = \"b\"\nval = 2\n",
    )
    .unwrap();
    let table = TomlReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.get(0, 0), Some(&CellValue::String("a".into())));
}

#[test]
fn toml_local_times_are_a_time_column_and_write_back_as_local_times() {
    let mut f = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    std::io::Write::write_all(
        &mut f,
        b"[[r]]\nat = 09:30:00\nmixed = 10:00:00\nname = \"09:30:00\"\n[[r]]\nat = 17:45:30.5\nmixed = \"later\"\nname = \"x\"\n",
    )
    .unwrap();
    let t = TomlReader.read_file(f.path()).unwrap();
    let col = |n: &str| t.columns.iter().position(|c| c.name == n).unwrap();
    assert!(t.columns[col("at")].data_type.starts_with("Time64"));
    assert_eq!(
        t.get(0, col("at")),
        Some(&CellValue::Time(34_200_000_000_000))
    );
    assert_eq!(t.columns[col("mixed")].data_type, "Utf8");
    assert_eq!(t.columns[col("name")].data_type, "Utf8");

    let out = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    TomlReader.write_file(out.path(), &t).unwrap();
    let text = std::fs::read_to_string(out.path()).unwrap();
    assert!(text.contains("at = 09:30:00"), "{text}");
    assert_eq!(TomlReader.read_file(out.path()).unwrap().rows, t.rows);
}

#[test]
fn a_time_column_with_an_empty_row_reads_back_as_time() {
    use octa::data::times::TimeKind;
    use octa::data::{ColumnInfo, DataTable};
    let t = DataTable {
        columns: vec![
            ColumnInfo {
                name: "at".into(),
                data_type: "Time64(Microsecond)".into(),
            },
            ColumnInfo {
                name: "n".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Time(52_200_000_000_000),
                CellValue::String("a".into()),
            ],
            vec![CellValue::Null, CellValue::String("b".into())],
            vec![
                CellValue::Time(21_600_000_000_000),
                CellValue::String("c".into()),
            ],
        ],
        ..DataTable::empty()
    };
    let out = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    TomlReader.write_file(out.path(), &t).unwrap();
    let back = TomlReader.read_file(out.path()).unwrap();
    let at = back.columns.iter().position(|c| c.name == "at").unwrap();
    assert_eq!(
        TimeKind::of_type(&back.columns[at].data_type),
        Some(TimeKind::Time),
        "type was {}",
        back.columns[at].data_type
    );
    let cells: Vec<&CellValue> = (0..back.rows.len())
        .map(|r| back.get(r, at).unwrap())
        .collect();
    assert_eq!(
        cells,
        vec![
            &CellValue::Time(52_200_000_000_000),
            &CellValue::Null,
            &CellValue::Time(21_600_000_000_000)
        ]
    );
}
