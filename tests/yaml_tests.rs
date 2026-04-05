use octo::data::CellValue;
use octo::formats::yaml_reader::*;
use octo::formats::FormatReader;
use serde_json::json;

#[test]
fn test_yaml_null() {
    assert_eq!(yaml_to_json(&serde_yaml::Value::Null), json!(null));
}

#[test]
fn test_yaml_bool() {
    assert_eq!(yaml_to_json(&serde_yaml::Value::Bool(true)), json!(true));
}

#[test]
fn test_yaml_number_int() {
    let v: serde_yaml::Value = serde_yaml::from_str("42").unwrap();
    assert_eq!(yaml_to_json(&v), json!(42));
}

#[test]
fn test_yaml_number_float() {
    let v: serde_yaml::Value = serde_yaml::from_str("3.14").unwrap();
    assert_eq!(yaml_to_json(&v), json!(3.14));
}

#[test]
fn test_yaml_string() {
    let v = serde_yaml::Value::String("hello".into());
    assert_eq!(yaml_to_json(&v), json!("hello"));
}

#[test]
fn test_yaml_sequence() {
    let v: serde_yaml::Value = serde_yaml::from_str("[1, 2, 3]").unwrap();
    assert_eq!(yaml_to_json(&v), json!([1, 2, 3]));
}

#[test]
fn test_yaml_mapping() {
    let v: serde_yaml::Value = serde_yaml::from_str("name: Alice\nage: 30").unwrap();
    let j = yaml_to_json(&v);
    assert_eq!(j["name"], json!("Alice"));
    assert_eq!(j["age"], json!(30));
}

#[test]
fn test_yaml_read_file() {
    let mut f = tempfile::NamedTempFile::with_suffix(".yaml").unwrap();
    std::io::Write::write_all(
        &mut f,
        b"- name: Alice\n  age: 30\n- name: Bob\n  age: 25\n",
    )
    .unwrap();
    let table = YamlReader.read_file(f.path()).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.get(0, 0), Some(&CellValue::String("Alice".into())));
    assert_eq!(table.get(1, 1), Some(&CellValue::Int(25)));
}
