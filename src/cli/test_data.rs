//! `octa --test-data FILE [FILE...] [--test-data-rows N] [--seed N]
//! [--test-data-out PATH] [--test-data-rename-categories]` - new rows shaped
//! like the real ones, to share instead of the data.
//!
//! Several files are generated together, so a link between them (an
//! `orders.customer_id` pointing at `customers.id`) still joins in the
//! output. One file writes to `--test-data-out FILE` or stdout; several need
//! `--test-data-out FOLDER` and each lands there as `<name>_test.<ext>`.
//!
//! The plan (which generator made each column) goes to stderr, and a note
//! names every column that keeps its real values (small categories), which
//! `--test-data-rename-categories` replaces with `value_1`, `value_2`, ...

use std::path::{Path, PathBuf};

use octa::data::DataTable;
use octa::data::test_data::{
    Generator, apply_links, generate, plan_table, profile_table, renamed_category, suggest_links,
};
use octa::formats::FormatRegistry;

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub inputs: Vec<PathBuf>,
    pub rows: Option<usize>,
    pub seed: u64,
    pub out: Option<PathBuf>,
    pub rename_categories: bool,
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "table".into())
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    if args.inputs.len() > 1 && args.out.as_ref().is_none_or(|o| !o.is_dir()) {
        anyhow::bail!(
            "several inputs need --test-data-out FOLDER (an existing folder); each file \
             is written there as <name>_test.<ext>"
        );
    }
    let tables: Vec<DataTable> = args
        .inputs
        .iter()
        .map(|p| super::read_table(p))
        .collect::<anyhow::Result<_>>()?;
    let mut plans: Vec<_> = tables
        .iter()
        .zip(&args.inputs)
        .map(|(t, p)| profile_table(t, &stem(p)))
        .collect();
    let refs: Vec<&DataTable> = tables.iter().collect();
    apply_links(&mut plans, &suggest_links(&refs));
    for plan in &mut plans {
        if let Some(n) = args.rows {
            plan.rows = n;
        }
        for col in &mut plan.columns {
            if let Generator::Category { values } = &col.generator
                && args.rename_categories
            {
                col.generator = renamed_category(values);
            }
            if col.generator.keeps_real_values() {
                eprintln!(
                    "note: {}.{} keeps its real values (a small category); pass \
                     --test-data-rename-categories to replace them",
                    plan.name, col.name
                );
            }
        }
    }
    let out = generate(&plans, args.seed)?;

    let plan_tsv = plan_table(&plans);
    for r in &plan_tsv.rows {
        eprintln!("{}.{}: {}", r[0], r[1], r[2]);
    }

    let registry = FormatRegistry::new();
    let write = |path: &Path, table: &DataTable| -> anyhow::Result<()> {
        let w = registry
            .reader_for_path(path)
            .filter(|w| w.supports_write())
            .ok_or_else(|| anyhow::anyhow!("cannot write {}", path.display()))?;
        w.write_file(path, table)?;
        eprintln!(
            "wrote {} rows x {} columns to {}",
            table.row_count(),
            table.col_count(),
            path.display()
        );
        Ok(())
    };
    match &args.out {
        None => write_table(&out[0], format)?,
        Some(dir) if dir.is_dir() => {
            for (table, input) in out.iter().zip(&args.inputs) {
                let ext = input
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "csv".into());
                write(&dir.join(format!("{}_test.{ext}", stem(input))), table)?;
            }
        }
        Some(file) => write(file, &out[0])?,
    }
    Ok(())
}
