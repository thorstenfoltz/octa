//! `octa --recipe RECIPE.ocp FILE [--recipe-out OUT]` - replay a saved recipe
//! on a file, for automation. The GUI's Apply recipe dialog runs the same
//! `octa::data::recipe::apply_recipe`.
//!
//! **All or nothing**: when any step cannot run (its column is gone), nothing
//! is written, the skipped steps go to stderr, and the exit code is 1. A
//! half-applied table silently passed down a pipeline is worse than a failed
//! run.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::recipe::{Recipe, apply_recipe};
use octa::formats::FormatRegistry;

use super::OutputFormat;
use super::output::write_table;

pub fn run(
    recipe: PathBuf,
    input: PathBuf,
    out: Option<PathBuf>,
    format: OutputFormat,
) -> anyhow::Result<ExitCode> {
    let recipe = Recipe::load(&recipe)?;
    let mut table = super::read_table(&input)?;
    let outcome = apply_recipe(&mut table, &recipe);

    let skipped: Vec<_> = outcome.iter().filter(|o| o.error.is_some()).collect();
    if !skipped.is_empty() {
        for (i, o) in outcome.iter().enumerate() {
            if let Some(e) = &o.error {
                eprintln!("step {}: {}: skipped, {e}", i + 1, o.description);
            }
        }
        eprintln!(
            "{} of {} step(s) could not run; nothing written",
            skipped.len(),
            outcome.len()
        );
        return Ok(ExitCode::FAILURE);
    }

    match out {
        Some(path) => {
            let registry = FormatRegistry::new();
            let writer = registry
                .reader_for_path(&path)
                .ok_or_else(|| anyhow::anyhow!("no writer for {}", path.display()))?;
            if !writer.supports_write() {
                anyhow::bail!("format {} does not support writing", writer.name());
            }
            writer.write_file(&path, &table)?;
            eprintln!(
                "applied {} step(s); wrote {} rows x {} columns to {}",
                outcome.len(),
                table.row_count(),
                table.col_count(),
                path.display()
            );
        }
        None => write_table(&table, format)?,
    }
    Ok(ExitCode::SUCCESS)
}
