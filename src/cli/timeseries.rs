//! `octa --resample COL --interval UNIT --value-cols A,B FILE`
//! `octa --rolling COL --order-by COL --window N FILE`
//!
//! Both build DuckDB SQL with the shared `octa::data::timeseries` builders (the
//! same ones the GUI dialog and the MCP tools use) and print the result through
//! the normal output formatter.

use std::path::PathBuf;

use octa::data::timeseries::{ResampleSpec, RollingSpec, build_resample_sql, build_rolling_sql};

use super::OutputFormat;
use super::output::write_table;

pub fn run_resample(path: PathBuf, spec: ResampleSpec, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let cols: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let sql = build_resample_sql(&spec, &cols)?;
    let outcome = octa::sql::run_query(&table, &sql)?;
    write_table(&outcome.table, format)
}

pub fn run_rolling(path: PathBuf, spec: RollingSpec, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let cols: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let sql = build_rolling_sql(&spec, &cols)?;
    let outcome = octa::sql::run_query(&table, &sql)?;
    write_table(&outcome.table, format)
}
