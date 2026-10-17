//! Hash columns: one new column holding a hex digest of chosen columns.
//!
//! The usual data-warehouse hash key, `MD5(UPPER(TRIM(a)) || '|' || b)`: the
//! cells of each row are turned into text whatever their type, prepared
//! (NULL text, trim, upper-case), joined with a free delimiter in the chosen
//! order and hashed. Pure, so the dialog, the MCP tool and the CLI action all
//! produce the same digest for the same row.
//!
//! The text of a cell is Octa's display text (`CellValue::to_string`). A
//! database renders dates and decimals its own way, so the same values can
//! hash differently there; the docs say so.

use md5::Md5;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};

use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HashColumnsAlgo {
    #[default]
    Md5,
    Sha256,
    Sha512,
}

impl HashColumnsAlgo {
    pub const ALL: [HashColumnsAlgo; 3] = [Self::Md5, Self::Sha256, Self::Sha512];

    /// Display name; the same in every language.
    pub fn label(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }

    /// `md5` / `sha256` / `sha512`, as the CLI and MCP spell it.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().replace('-', "").as_str() {
            "md5" => Some(Self::Md5),
            "sha256" => Some(Self::Sha256),
            "sha512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn hex(self, bytes: &[u8]) -> String {
        match self {
            Self::Md5 => hex(&Md5::digest(bytes)),
            Self::Sha256 => hex(&Sha256::digest(bytes)),
            Self::Sha512 => hex(&Sha512::digest(bytes)),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HashColumnsSpec {
    /// Column indices, in the order they are joined.
    pub columns: Vec<usize>,
    pub algo: HashColumnsAlgo,
    /// Put between the values. May be empty.
    pub delimiter: String,
    /// Stands in for a NULL cell.
    pub null_text: String,
    /// Strip leading and trailing whitespace from each value.
    pub trim: bool,
    /// Upper-case each value before hashing.
    pub upper: bool,
}

impl Default for HashColumnsSpec {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            algo: HashColumnsAlgo::Md5,
            delimiter: "|".to_string(),
            null_text: String::new(),
            trim: false,
            upper: false,
        }
    }
}

/// The text one row hashes, before hashing. Exposed for the dialog's preview
/// tooltip and for tests.
pub fn row_input(table: &DataTable, row: usize, spec: &HashColumnsSpec) -> String {
    spec.columns
        .iter()
        .map(|&c| {
            let text = match table.get(row, c) {
                None | Some(CellValue::Null) => spec.null_text.clone(),
                Some(v) => v.to_string(),
            };
            let text = if spec.trim {
                text.trim().to_string()
            } else {
                text
            };
            if spec.upper {
                text.to_uppercase()
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(&spec.delimiter)
}

/// One lowercase hex digest per row, as `CellValue::String`.
pub fn hash_columns(table: &DataTable, spec: &HashColumnsSpec) -> Vec<CellValue> {
    (0..table.row_count())
        .map(|r| CellValue::String(hash_columns_row(table, r, spec)))
        .collect()
}

/// The digest of one row, for the dialog's preview.
pub fn hash_columns_row(table: &DataTable, row: usize, spec: &HashColumnsSpec) -> String {
    spec.algo.hex(row_input(table, row, spec).as_bytes())
}

/// Default name for the new column: `hash_<c0>_<c1>...`.
pub fn default_name(table: &DataTable, columns: &[usize]) -> String {
    let parts: Vec<&str> = columns
        .iter()
        .filter_map(|&c| table.columns.get(c).map(|x| x.name.as_str()))
        .collect();
    format!("hash_{}", parts.join("_"))
}

/// Resolve `names`, hash, append the column (no undo: for the CLI and MCP,
/// which write a file; the dialog goes through `insert_column`). Returns the
/// new column's name.
pub fn add_hash_column(
    table: &mut DataTable,
    names: &[String],
    mut spec: HashColumnsSpec,
    new_column: Option<&str>,
) -> anyhow::Result<String> {
    if names.is_empty() {
        anyhow::bail!("columns must not be empty");
    }
    spec.columns = names
        .iter()
        .map(|n| {
            table
                .columns
                .iter()
                .position(|c| &c.name == n)
                .ok_or_else(|| anyhow::anyhow!("no such column: {n}"))
        })
        .collect::<anyhow::Result<_>>()?;
    let name = new_column
        .map(str::to_string)
        .unwrap_or_else(|| default_name(table, &spec.columns));
    if table.columns.iter().any(|c| c.name == name) {
        anyhow::bail!("a column named {name} already exists; pick another new_column");
    }
    let values = hash_columns(table, &spec);
    table.columns.push(ColumnInfo {
        name: name.clone(),
        data_type: "Utf8".into(),
    });
    for (row, v) in table.rows.iter_mut().zip(values) {
        row.push(v);
    }
    Ok(name)
}

#[cfg(test)]
#[path = "hash_columns_tests.rs"]
mod tests;
