//! `octa --calculate KIND --calc-column COL FILE` - add a rank, row number,
//! running total, percent of total, change-to-previous, previous / next
//! value, visit number or ABC class column and print the table. Never modifies FILE. Same engine as the GUI dialog and the MCP
//! tool `calculate_column`.

use std::path::PathBuf;

use octa::data::calculated::{CalcByName, CalcExtras, CalcKind, add_calculated_column};

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub kind: String,
    pub column: Option<String>,
    pub order_by: Option<String>,
    pub descending: Option<bool>,
    pub groups: Option<String>,
    pub name: Option<String>,
    pub extras: CalcExtras,
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let kind = CalcKind::parse(&o.kind).ok_or_else(|| {
        let ids: Vec<&str> = CalcKind::ALL.iter().map(|k| k.id()).collect();
        anyhow::anyhow!(
            "--calculate: unknown \"{}\"; expected one of {}",
            o.kind,
            ids.join(", ")
        )
    })?;
    let mut table = super::read_table(&path)?;
    let by = CalcByName {
        kind,
        value: o.column,
        order: o.order_by,
        descending: o.descending,
        groups: o
            .groups
            .map(|g| {
                g.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        extras: o.extras,
    };
    let spec = by
        .resolve(&table)
        .map_err(|e| anyhow::anyhow!("--calculate: {e:#}"))?;
    add_calculated_column(&mut table, &spec, o.name.as_deref())
        .map_err(|e| anyhow::anyhow!("--calculate: {e:#}"))?;
    write_table(&table, format)
}
