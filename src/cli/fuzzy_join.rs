//! `octa --fuzzy-join FILE --fuzzy-join-file FILE --fuzzy-on LEFT=RIGHT`
//!
//! Joins tables on similarity rather than equality, for the case where two
//! systems name the same thing differently: `Mueller GmbH` against
//! `Mueller Gmbh.`. The result table goes to stdout; the per-step report
//! (rows compared, matched, ambiguous, capped) goes to stderr so a pipe stays
//! parseable.
//!
//! The engine is `octa::data::fuzzy_join`, shared with the GUI dialog and the
//! `fuzzy_join` MCP tool.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use octa::data::DataTable;
use octa::data::fuzzy_duplicates::{NormalizeOpts, SimilarityMethod};
use octa::data::fuzzy_join::{FuzzyJoinStep, fuzzy_join};
use octa::data::join::JoinType;

use super::OutputFormat;
use super::output::write_table;

/// The flags for one run, bundled so `run` takes two arguments rather than
/// nine.
#[derive(Debug)]
pub struct Args {
    pub files: Vec<PathBuf>,
    pub join_file: Vec<PathBuf>,
    pub on: Vec<String>,
    pub method: String,
    pub threshold: f64,
    pub block: Option<String>,
    pub join_type: String,
    pub max_rows: usize,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    if args.join_file.is_empty() {
        anyhow::bail!(
            "--fuzzy-join requires at least one --fuzzy-join-file PATH beside the positional file"
        );
    }
    if args.on.is_empty() {
        anyhow::bail!("--fuzzy-join requires --fuzzy-on LEFT=RIGHT with at least one column pair");
    }
    let Some(first) = args.files.first().cloned() else {
        anyhow::bail!("--fuzzy-join needs an input FILE");
    };

    let all_paths: Vec<PathBuf> = std::iter::once(first).chain(args.join_file).collect();
    let tables: Vec<DataTable> = all_paths
        .iter()
        .map(|p| super::read_table(p))
        .collect::<anyhow::Result<_>>()?;

    let method = parse_method(&args.method)?;
    let how = parse_join_type(&args.join_type)?;

    // One step per extra table. The same column pairs, measure and threshold
    // apply to every step: the CLI takes one setting, and a run that needs
    // different ones per step is a job for the dialog.
    let mut steps = Vec::new();
    for i in 0..tables.len() - 1 {
        let left = &tables[i];
        let right = &tables[i + 1];
        let pairs = args
            .on
            .iter()
            .map(|spec| resolve_pair(spec, left, right))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let block = match &args.block {
            Some(spec) => Some(resolve_pair(spec, left, right)?),
            None => None,
        };
        steps.push(FuzzyJoinStep {
            pairs,
            method,
            threshold: args.threshold,
            normalize: NormalizeOpts::default(),
            block,
            how,
            max_rows: args.max_rows,
        });
    }

    let refs: Vec<&DataTable> = tables.iter().collect();
    let result = fuzzy_join(&refs, &steps, &AtomicBool::new(false))?;

    for (i, r) in result.steps.iter().enumerate() {
        eprintln!(
            "step {}: {} x {} rows compared, {} matched, {} ambiguous{}",
            i + 1,
            r.left_rows,
            r.right_rows,
            r.matched,
            r.ambiguous,
            if r.capped { " (row cap hit)" } else { "" }
        );
    }

    write_table(&result.table, format)
}

/// Resolve `LEFT=RIGHT` against two tables, naming whichever side is missing.
fn resolve_pair(spec: &str, left: &DataTable, right: &DataTable) -> anyhow::Result<(usize, usize)> {
    let Some((l, r)) = spec.split_once('=') else {
        anyhow::bail!("expected LEFT=RIGHT, got {spec:?}");
    };
    let li = column_index(left, l.trim())?;
    let ri = column_index(right, r.trim())?;
    Ok((li, ri))
}

fn column_index(t: &DataTable, name: &str) -> anyhow::Result<usize> {
    t.columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "column {name:?} not found; available: {}",
                t.columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn parse_method(s: &str) -> anyhow::Result<SimilarityMethod> {
    match s.trim().to_ascii_lowercase().as_str() {
        "edit_ratio" | "edit" => Ok(SimilarityMethod::EditRatio),
        "jaro_winkler" | "jaro" => Ok(SimilarityMethod::JaroWinkler),
        "token_set" | "token" => Ok(SimilarityMethod::TokenSet),
        other => anyhow::bail!(
            "unknown --fuzzy-method {other:?}; valid: edit_ratio, jaro_winkler, token_set"
        ),
    }
}

fn parse_join_type(s: &str) -> anyhow::Result<JoinType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "left" => Ok(JoinType::Left),
        "inner" => Ok(JoinType::Inner),
        "right" => Ok(JoinType::Right),
        "full" => Ok(JoinType::Full),
        other => {
            anyhow::bail!("unknown --fuzzy-join-type {other:?}; valid: inner, left, right, full")
        }
    }
}
