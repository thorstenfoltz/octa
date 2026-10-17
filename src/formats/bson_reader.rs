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
#[path = "bson_reader_tests.rs"]
mod tests;
