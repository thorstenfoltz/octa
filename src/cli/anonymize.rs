//! `octa --anonymize SPEC FILE` - sanitise columns per a JSON spec file and
//! print the result. Never mutates the input file in place (always emits the
//! new-table form). The spec mirrors [`octa::data::transform::AnonSpec`] but
//! addresses columns by **name**; names are resolved to indices here.

use std::path::PathBuf;

use serde::Deserialize;

use octa::data::transform::{AnonRule, AnonSource, AnonSpec, AnonStrategy, anonymize_table};
use octa::data::{CellValue, ColumnInfo, DataTable};

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
    rules: Vec<NamedRule>,
}

#[derive(Debug, Deserialize)]
struct NamedRule {
    /// One column name or an array of names.
    columns: ColumnsField,
    #[serde(default)]
    new_column: Option<String>,
    strategy: AnonStrategy,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ColumnsField {
    One(String),
    Many(Vec<String>),
}

impl ColumnsField {
    fn names(&self) -> Vec<String> {
        match self {
            ColumnsField::One(s) => vec![s.clone()],
            ColumnsField::Many(v) => v.clone(),
        }
    }
}

pub fn run(spec_path: PathBuf, file: PathBuf, format: OutputFormat) -> anyhow::Result<()> {
    let spec_text = std::fs::read_to_string(&spec_path)
        .map_err(|e| anyhow::anyhow!("reading spec {}: {e}", spec_path.display()))?;
    let named: NamedSpec = serde_json::from_str(&spec_text)
        .map_err(|e| anyhow::anyhow!("parsing spec {}: {e}", spec_path.display()))?;
    let new_columns = matches!(named.output.as_deref(), Some("new_columns"));

    let mut table = super::read_table(&file)?;

    // Resolve column names to indices.
    let mut rules = Vec::with_capacity(named.rules.len());
    for r in &named.rules {
        let mut cols = Vec::new();
        for n in r.columns.names() {
            let idx = table
                .columns
                .iter()
                .position(|c| c.name == n)
                .ok_or_else(|| anyhow::anyhow!("no such column: {n}"))?;
            cols.push(idx);
        }
        rules.push(AnonRule {
            columns: cols,
            strategy: r.strategy.clone(),
            new_column: r.new_column.clone(),
        });
    }
    let outputs = anonymize_table(
        &table,
        &AnonSpec {
            rules,
            salt: named.salt,
        },
    );

    for o in outputs {
        match o.source {
            AnonSource::Column(c) if !new_columns => {
                for (row, v) in o.values.into_iter().enumerate() {
                    table.set(row, c, v);
                }
            }
            AnonSource::Column(c) => {
                let base = table
                    .columns
                    .get(c)
                    .map(|x| x.name.clone())
                    .unwrap_or_default();
                append_column(&mut table, &format!("{base}_anon"), o.values);
            }
            AnonSource::Derived { name } => append_column(&mut table, &name, o.values),
        }
    }
    table.apply_edits();
    write_table(&table, format)?;
    Ok(())
}

/// Append a new Utf8 column with the given values (uniquifying the name).
fn append_column(table: &mut DataTable, name: &str, values: Vec<CellValue>) {
    let mut unique = name.to_string();
    let mut k = 2;
    while table.columns.iter().any(|c| c.name == unique) {
        unique = format!("{name}_{k}");
        k += 1;
    }
    table.columns.push(ColumnInfo {
        name: unique,
        data_type: "Utf8".into(),
    });
    for (row, v) in values.into_iter().enumerate() {
        if let Some(r) = table.rows.get_mut(row) {
            r.push(v);
        }
    }
}
