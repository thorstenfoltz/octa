use octo::data::CellValue;
use octo::formats::toml_reader::*;
use octo::formats::FormatReader;
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
    let v = toml::Value::Float(3.14);
    assert_eq!(toml_to_json(&v), json!(3.14));
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
