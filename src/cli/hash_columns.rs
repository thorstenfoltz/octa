//! `octa --hash-columns COL,COL,... FILE` - append a column holding an MD5 /
//! SHA-256 / SHA-512 hex digest of the named columns and print the table.
//! Never modifies FILE. Same engine as the GUI dialog and the MCP tool.

use std::path::PathBuf;

use octa::data::transform::hash_columns::{HashColumnsAlgo, HashColumnsSpec, add_hash_column};

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub columns: String,
    pub algo: Option<String>,
    pub delimiter: Option<String>,
    pub null_text: Option<String>,
    pub trim: bool,
    pub upper: bool,
    pub name: Option<String>,
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let algo = match o.algo.as_deref() {
        None => HashColumnsAlgo::Md5,
        Some(s) => HashColumnsAlgo::parse(s).ok_or_else(|| {
            anyhow::anyhow!("--hash-algo: unknown \"{s}\"; expected md5, sha256 or sha512")
        })?,
    };
    let names: Vec<String> = o
        .columns
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut table = super::read_table(&path)?;
    let spec = HashColumnsSpec {
        columns: Vec::new(),
        algo,
        delimiter: o.delimiter.unwrap_or_else(|| "|".to_string()),
        null_text: o.null_text.unwrap_or_default(),
        trim: o.trim,
        upper: o.upper,
    };
    add_hash_column(&mut table, &names, spec, o.name.as_deref())
        .map_err(|e| anyhow::anyhow!("--hash-columns: {e:#}"))?;
    write_table(&table, format)
}
