//! `octa --classify QUESTION --classify-columns COLS FILE` - ask TypeSafe AI's
//! Jev model the question about every row and print the table with the answer
//! and its certainty as two new columns. Never modifies FILE. Same engine as
//! the GUI dialog and the MCP tool `classify_rows`.
//!
//! This sends cell data to TypeSafe. It refuses until the feature is switched
//! on and a key is set (`JevConfig::resolve` says how).

use std::path::PathBuf;

use octa::api::jev::JevConfig;
use octa::data::transform::classify::{
    Category, ClassifyMode, ClassifySpec, RunReport, add_classify_columns,
};
use octa::ui::settings::AppSettings;

use super::OutputFormat;
use super::output::write_table;

pub struct Options {
    pub question: String,
    /// Comma list of column names.
    pub columns: Option<String>,
    /// `NAME` or `NAME=DESCRIPTION`, one per flag.
    pub categories: Vec<String>,
    /// Comma list, lowest level first.
    pub levels: Option<String>,
    pub name: Option<String>,
}

fn comma_list(s: Option<String>) -> Vec<String> {
    s.map(|s| {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// The spec the flags describe, columns left for the engine to resolve.
/// Categories make it a choice, levels a scale, neither a yes/no question.
pub fn spec_from(o: &Options) -> anyhow::Result<ClassifySpec> {
    let levels = comma_list(o.levels.clone());
    let mode = match (o.categories.is_empty(), levels.is_empty()) {
        (false, false) => {
            anyhow::bail!("give --classify-category or --classify-levels, not both")
        }
        (false, true) => ClassifyMode::Categories,
        (true, false) => ClassifyMode::Scale,
        (true, true) => ClassifyMode::YesNo,
    };
    Ok(ClassifySpec {
        columns: Vec::new(),
        mode,
        instructions: o.question.clone(),
        categories: o
            .categories
            .iter()
            .map(|c| match c.split_once('=') {
                Some((name, description)) => Category {
                    name: name.trim().to_string(),
                    description: description.trim().to_string(),
                },
                None => Category {
                    name: c.trim().to_string(),
                    description: String::new(),
                },
            })
            .collect(),
        levels,
    })
}

/// One line for stderr: what was sent and what came back.
pub fn report_line(r: &RunReport) -> String {
    let mut line = format!(
        "{} distinct values sent to TypeSafe, {} input tokens",
        r.sent, r.input_tokens
    );
    if r.failed > 0 {
        line.push_str(&format!(
            "; {} got no answer and their rows stay empty",
            r.failed
        ));
        if let Some(e) = &r.first_error {
            line.push_str(&format!(" (first error: {e})"));
        }
    }
    line
}

pub fn run(path: PathBuf, o: Options, format: OutputFormat) -> anyhow::Result<()> {
    let spec = spec_from(&o).map_err(|e| anyhow::anyhow!("--classify: {e:#}"))?;
    let names = comma_list(o.columns.clone());
    let config = JevConfig::resolve(&AppSettings::load())?;
    let mut table = super::read_table(&path)?;
    let send = config.sender();
    let (_, report) = add_classify_columns(
        &mut table,
        &names,
        spec,
        o.name.as_deref(),
        &config.model,
        config.parallel,
        &send,
    )
    .map_err(|e| anyhow::anyhow!("--classify: {e:#}"))?;
    eprintln!("{}", report_line(&report));
    write_table(&table, format)
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
