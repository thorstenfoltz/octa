//! MessagePack reader (read-only).
//!
//! Decodes a MessagePack document to a `serde_json::Value` and hands it to the
//! shared [`json_reader::json_to_table`] flattener, so nested maps become
//! dotted columns exactly like JSON. A top-level array of maps becomes rows; a
//! single map becomes a one-row table.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::data::DataTable;
use crate::formats::FormatReader;
use crate::formats::json_reader::json_to_table;

pub struct MsgpackReader;

impl FormatReader for MsgpackReader {
    fn name(&self) -> &str {
        "MessagePack"
    }

    fn extensions(&self) -> &[&str] {
        &["msgpack", "mpk"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading MessagePack file {}", path.display()))?;
        let value: Value =
            rmp_serde::from_slice(&bytes).context("decoding MessagePack into a value")?;
        json_to_table(value, path, "MessagePack")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::CellValue;
    use std::io::Write;

    /// Encode a serde_json value as MessagePack into a temp file and read it
    /// back through the reader.
    fn roundtrip(value: Value) -> DataTable {
        let bytes = rmp_serde::to_vec(&value).expect("encode msgpack");
        let mut tmp = tempfile::NamedTempFile::with_suffix(".msgpack").expect("tmp");
        tmp.write_all(&bytes).expect("write");
        MsgpackReader.read_file(tmp.path()).expect("read msgpack")
    }

    #[test]
    fn array_of_maps_becomes_rows() {
        let table = roundtrip(serde_json::json!([
            {"id": 1, "name": "a"},
            {"id": 2, "name": "b"},
        ]));
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.col_count(), 2);
        assert_eq!(table.format_name.as_deref(), Some("MessagePack"));
        let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"id"));
        assert!(names.contains(&"name"));
    }

    #[test]
    fn nested_map_flattens_to_dotted_columns() {
        let table = roundtrip(serde_json::json!([{"a": {"b": 7}}]));
        assert_eq!(table.row_count(), 1);
        let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"a.b"), "{names:?}");
        let col = names.iter().position(|n| *n == "a.b").unwrap();
        assert_eq!(table.get(0, col), Some(&CellValue::Int(7)));
    }
}
