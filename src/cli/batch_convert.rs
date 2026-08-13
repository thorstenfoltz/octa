//! `octa --batch-convert --to EXT --out-dir DIR FILE...`
//!
//! Converts every input into one target format in a single run. Prints a bare
//! `input TAB output TAB status` listing, headerless, exactly as
//! `--partition-by` prints its file list, and like that action ignores the
//! global `-f / --format`: this is a record of work done, not a data table.
//!
//! Returns an `ExitCode` rather than `()` because a run where some items failed
//! is a successful read with a non-zero exit, the same shape `--validate-schema`
//! needs.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use octa::data::batch_convert::{BatchStatus, plan_batch, run_batch};

pub fn run(
    inputs: Vec<PathBuf>,
    out_dir: PathBuf,
    target_ext: String,
    overwrite: bool,
    opts: octa::formats::write_options::WriteOptions,
) -> anyhow::Result<ExitCode> {
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        anyhow::anyhow!(
            "could not create output directory {}: {e}",
            out_dir.display()
        )
    })?;

    let plan = plan_batch(&inputs, &out_dir, &target_ext, overwrite);
    let report = run_batch(plan, &|_, _| {}, &AtomicBool::new(false), &opts);

    for item in &report.items {
        let status = match &item.status {
            BatchStatus::Done { .. } => "done",
            BatchStatus::Failed(_) => "failed",
            BatchStatus::Skipped(_) => "skipped",
            BatchStatus::Pending => "pending",
        };
        println!(
            "{}\t{}\t{status}",
            item.input.display(),
            item.output.display()
        );
    }

    // Failure detail and the summary go to stderr so the stdout listing stays
    // machine-readable.
    for item in &report.items {
        if let BatchStatus::Failed(e) = &item.status {
            eprintln!("{}: {e}", item.input.display());
        }
    }
    eprintln!(
        "{} converted, {} failed, {} skipped",
        report.converted, report.failed, report.skipped
    );

    Ok(if report.failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
