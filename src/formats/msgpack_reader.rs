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
#[path = "msgpack_reader_tests.rs"]
mod tests;
