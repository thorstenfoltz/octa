//! BSON reader (read-only).
//!
//! A `.bson` file is one or more BSON documents concatenated back-to-back
//! (the shape `mongodump` produces). Each document is decoded and converted to
//! relaxed extended JSON, then the whole sequence is handed to the shared
//! [`json_reader::json_to_table`] flattener, so MongoDB dumps open as a table
//! with one row per document and nested fields flattened to dotted columns.

use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use bson::{Bson, Document};
use serde_json::Value;

use crate::data::DataTable;
use crate::formats::FormatReader;
use crate::formats::json_reader::json_to_table;

pub struct BsonReader;

impl FormatReader for BsonReader {
    fn name(&self) -> &str {
        "BSON"
    }

    fn extensions(&self) -> &[&str] {
        &["bson"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let bytes =
            std::fs::read(path).with_context(|| format!("reading BSON file {}", path.display()))?;
        let docs = read_documents(&bytes)?;
        json_to_table(Value::Array(docs), path, "BSON")
    }
}

/// Decode every BSON document in `bytes` into a relaxed-extjson value. Stops
/// cleanly at the end of the buffer; an incomplete trailing document is an
/// error.
fn read_documents(bytes: &[u8]) -> Result<Vec<Value>> {
    let mut cursor = Cursor::new(bytes);
    let mut docs = Vec::new();
    while (cursor.position() as usize) < bytes.len() {
        let doc = Document::from_reader(&mut cursor).context("decoding BSON document")?;
        docs.push(Bson::Document(doc).into_relaxed_extjson());
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
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
}
