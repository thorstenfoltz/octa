//! `octa --anonymize SPEC FILE` - sanitise columns per a JSON spec file and
//! print the result. Never mutates the input file in place (always emits the
//! new-table form). The spec mirrors [`octa::data::transform::AnonSpec`] but
//! addresses columns by **name**; [`anonymize_by_name`] resolves them.

use std::path::PathBuf;

use serde::Deserialize;

use octa::data::transform::{NamedAnonRule, anonymize_by_name};

use super::OutputFormat;
use super::output::write_table;

/// JSON spec shape: like `AnonSpec`, but `columns` are names and an optional
/// `output` mode selects in-place vs new columns.
#[derive(Debug, Deserialize)]
struct NamedSpec {
    #[serde(default)]
    salt: String,
    /// `in_place` (default) or `new_columns`.
    #[serde(default)]
    output: Option<String>,
    rules: Vec<NamedAnonRule>,
}

pub fn run(spec_path: PathBuf, file: PathBuf, format: OutputFormat) -> anyhow::Result<()> {
    let spec_text = std::fs::read_to_string(&spec_path)
        .map_err(|e| anyhow::anyhow!("reading spec {}: {e}", spec_path.display()))?;
    let named: NamedSpec = serde_json::from_str(&spec_text)
        .map_err(|e| anyhow::anyhow!("parsing spec {}: {e}", spec_path.display()))?;
    let new_columns = matches!(named.output.as_deref(), Some("new_columns"));

    let mut table = super::read_table(&file)?;
    anonymize_by_name(&mut table, &named.rules, &named.salt, new_columns)?;
    write_table(&table, format)?;
    Ok(())
}
