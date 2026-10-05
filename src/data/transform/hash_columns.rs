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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = cols
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).into(),
                data_type: "Utf8".into(),
            })
            .collect();
        t.rows = rows;
        t
    }

    fn s(v: &str) -> CellValue {
        CellValue::String(v.into())
    }

    fn spec(cols: Vec<usize>, algo: HashColumnsAlgo) -> HashColumnsSpec {
        HashColumnsSpec {
            columns: cols,
            algo,
            ..HashColumnsSpec::default()
        }
    }

    /// Known answers, checked against `printf 'a|b' | md5sum` and friends.
    #[test]
    fn known_digests() {
        let t = table(&["x", "y"], vec![vec![s("a"), s("b")]]);
        let h = |algo| hash_columns(&t, &spec(vec![0, 1], algo))[0].to_string();
        assert_eq!(h(HashColumnsAlgo::Md5), "d0726241020676b14aa6298ce6a18b21");
        assert_eq!(
            h(HashColumnsAlgo::Sha256),
            "0eab8a0a3380abf4c7d1fb0b43b66aafbb64a4b953e4eb2dccca579461912d0c"
        );
        let sha512 = h(HashColumnsAlgo::Sha512);
        assert_eq!(sha512.len(), 128);
        assert!(sha512.starts_with("3c075e5f72e4ec6eeaf0"), "{sha512}");
    }

    #[test]
    fn the_empty_string_hashes_to_the_textbook_value() {
        let t = table(&["x"], vec![vec![CellValue::Null]]);
        let out = hash_columns(&t, &spec(vec![0], HashColumnsAlgo::Sha256));
        assert_eq!(
            out[0].to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn order_delimiter_null_trim_and_upper_shape_the_input() {
        let t = table(
            &["a", "b", "c"],
            vec![vec![s(" x "), CellValue::Null, s("y")]],
        );
        let mut sp = spec(vec![2, 0, 1], HashColumnsAlgo::Md5);
        assert_eq!(row_input(&t, 0, &sp), "y| x |");
        sp.delimiter = ";;".into();
        sp.null_text = "<NULL>".into();
        sp.trim = true;
        sp.upper = true;
        assert_eq!(row_input(&t, 0, &sp), "Y;;X;;<NULL>");
        sp.delimiter.clear();
        assert_eq!(row_input(&t, 0, &sp), "YX<NULL>");
    }

    /// Any type goes in as its display text.
    #[test]
    fn mixed_types_are_hashed_through_their_text() {
        let t = table(
            &["i", "f", "d", "b", "n"],
            vec![vec![
                CellValue::Int(42),
                CellValue::Float(1.5),
                CellValue::Date("2026-10-01".into()),
                CellValue::Bool(true),
                CellValue::Null,
            ]],
        );
        let sp = spec(vec![0, 1, 2, 3, 4], HashColumnsAlgo::Md5);
        assert_eq!(row_input(&t, 0, &sp), "42|1.5|2026-10-01|true|");
        assert_eq!(hash_columns(&t, &sp)[0].to_string().len(), 32);
    }

    #[test]
    fn algo_names_parse_with_or_without_the_dash() {
        assert_eq!(
            HashColumnsAlgo::parse("SHA-256"),
            Some(HashColumnsAlgo::Sha256)
        );
        assert_eq!(
            HashColumnsAlgo::parse("sha512"),
            Some(HashColumnsAlgo::Sha512)
        );
        assert_eq!(HashColumnsAlgo::parse("md5"), Some(HashColumnsAlgo::Md5));
        assert_eq!(HashColumnsAlgo::parse("sha1"), None);
    }

    #[test]
    fn add_hash_column_resolves_names_and_refuses_a_taken_name() {
        let mut t = table(&["x", "y"], vec![vec![s("a"), s("b")]]);
        let name = add_hash_column(
            &mut t,
            &["x".into(), "y".into()],
            HashColumnsSpec::default(),
            None,
        )
        .unwrap();
        assert_eq!(name, "hash_x_y");
        assert_eq!(t.rows[0][2].to_string(), "d0726241020676b14aa6298ce6a18b21");
        assert!(
            add_hash_column(&mut t, &["x".into()], HashColumnsSpec::default(), Some("x")).is_err()
        );
        assert!(
            add_hash_column(&mut t, &["nope".into()], HashColumnsSpec::default(), None).is_err()
        );
    }

    #[test]
    fn the_default_name_joins_the_column_names() {
        let t = table(&["id", "email"], vec![]);
        assert_eq!(default_name(&t, &[1, 0]), "hash_email_id");
    }
}
