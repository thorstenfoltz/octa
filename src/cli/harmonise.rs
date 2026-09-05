//! `--harmonise-schema DIR --out-dir DIR`: rewrite a folder of drifting files
//! to one common schema.
//!
//! The write half of `--schema-drift`. Exits 1 when any file was refused,
//! joining `--validate-schema`, `--schema-drift` and `--batch-convert`: a run
//! where some files could not be harmonised is a successful read with a
//! non-zero outcome, which a CI step should be able to gate on.
//!
//! The report goes to stdout through the normal formatter; refusals and the
//! summary go to stderr so a pipe stays parseable.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::harmonise::{
    FileStatus, HarmoniseOptions, plan_harmonise, report_table, run_harmonise,
};
use octa::data::schema_drift::{DriftOptions, analyse, collect_schemas};
use octa::formats::FormatRegistry;

use super::OutputFormat;
use super::output::write_table;
use super::progress::{Progress, short_name};

/// The flags for one run, bundled so the entry point stays under clippy's
/// argument-count threshold, matching `cli::fuzzy_join::Args`.
pub struct Args {
    pub dir: PathBuf,
    pub out_dir: PathBuf,
    pub target_file: Option<PathBuf>,
    pub recursive: bool,
    pub ignore_case: bool,
    pub overwrite: bool,
}

pub fn run(
    args: Args,
    format: OutputFormat,
    write_opts: &octa::formats::write_options::WriteOptions,
) -> anyhow::Result<ExitCode> {
    let Args {
        dir,
        out_dir,
        target_file,
        recursive,
        ignore_case,
        overwrite,
    } = args;
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }
    if out_dir == dir {
        anyhow::bail!(
            "--out-dir must differ from the scanned folder: harmonised copies never overwrite the originals"
        );
    }
    let registry = FormatRegistry::new();
    let (files, skipped) = collect_schemas(&dir, recursive, &registry);
    if files.is_empty() {
        anyhow::bail!("no readable files in {}", dir.display());
    }

    // The target schema: either a file the user named, or the shape most of
    // the folder already has (which is the one needing fewest rewrites).
    let target = match &target_file {
        Some(p) => {
            octa::formats::read_table_auto(
                p,
                None,
                octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
            )
            .map_err(|e| anyhow::anyhow!("could not read --target-file {}: {e}", p.display()))?
            .columns
        }
        None => {
            let report = analyse(
                &files,
                &DriftOptions {
                    ignore_case,
                    recursive,
                },
            );
            report
                .variants
                .first()
                .map(|v| v.columns.clone())
                .unwrap_or_default()
        }
    };

    let opts = HarmoniseOptions {
        root: dir.clone(),
        out_dir,
        ignore_case,
        overwrite,
    };
    let plan = plan_harmonise(&files, &target, &opts);
    let bar = std::cell::RefCell::new(Progress::start(Some(plan.actions.len())));
    let report = run_harmonise(
        &plan,
        &opts,
        // `run_harmonise` reports `done + 1` while walking `plan.actions` in
        // order, so index `done` names the file it just finished.
        &|done, _| {
            let name = plan
                .actions
                .get(done - 1)
                .map(|a| short_name(&a.input))
                .unwrap_or_default();
            bar.borrow_mut().item(done, &name);
        },
        &std::sync::atomic::AtomicBool::new(false),
        write_opts,
    );
    bar.borrow_mut().finish();

    write_table(&report_table(&report), format)?;

    for (file, reason) in &skipped {
        eprintln!("skipped {file}: {reason}");
    }
    for item in &report.items {
        if let FileStatus::Refused(why) = &item.status {
            eprintln!("refused {}: {why}", item.input.display());
        }
        if !item.dropped.is_empty() {
            eprintln!(
                "dropped from {}: {}",
                item.input.display(),
                item.dropped.join(", ")
            );
        }
    }
    eprintln!(
        "harmonised {} file(s), refused {}",
        report.written, report.refused
    );
    if report.refused > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}
