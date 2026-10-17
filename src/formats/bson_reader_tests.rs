use super::*;
use crate::data::CellValue;
use std::io::Write;

fn write_docs(docs: &[Document]) -> tempfile::NamedTempFile {
    let mut tmp = tempfile::NamedTempFile::with_suffix(".bson").expect("tmp");
    for doc in docs {
        let mut buf = Vec::new();
        doc.to_writer(&mut buf).expect("encode bson");
        tmp.write_all(&buf).expect("write");
    }
    tmp
}

#[test]
fn reads_concatenated_documents_as_rows() {
    let docs = vec![
        bson::doc! {"id": 1_i32, "name": "a"},
        bson::doc! {"id": 2_i32, "name": "b"},
    ];
    let tmp = write_docs(&docs);
    let table = BsonReader.read_file(tmp.path()).expect("read bson");
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.format_name.as_deref(), Some("BSON"));
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"id"), "{names:?}");
    assert!(names.contains(&"name"), "{names:?}");
}

#[test]
fn nested_document_flattens() {
    let docs = vec![bson::doc! {"outer": {"inner": 42_i32}}];
    let tmp = write_docs(&docs);
    let table = BsonReader.read_file(tmp.path()).expect("read bson");
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    let col = names
        .iter()
        .position(|n| *n == "outer.inner")
        .unwrap_or_else(|| panic!("no outer.inner in {names:?}"));
    assert_eq!(table.get(0, col), Some(&CellValue::Int(42)));
}
