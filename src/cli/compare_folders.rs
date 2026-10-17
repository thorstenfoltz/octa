//! `octa --compare-folders A B [C ...]`: N folders, file by file.
//!
//! Engine: `octa::data::folder_compare`. Exit 1 when anything differs, is
//! missing, or a folder hit the file limit (an incomplete comparison is not
//! a pass); 0 when every file is the same everywhere.

use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use octa::data::folder_compare::any_difference;
use octa::data::folder_compare::scan::{ScanProgress, compare_sources, source_for};

use super::OutputFormat;
use super::output::write_table;
use super::progress::Progress;

pub fn run(roots: Vec<String>, recursive: bool, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let settings = octa::ui::settings::AppSettings::load();
    let cap = settings.folder_compare_cap();
    let sources = roots
        .iter()
        .map(|r| Ok((r.clone(), source_for(r, &settings)?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut bar: Option<Progress> = None;
    let (table, listings) = compare_sources(
        sources,
        recursive,
        cap,
        &AtomicBool::new(false),
        &mut |p| match p {
            ScanProgress::Listing(label) => eprintln!("listing {label}"),
            ScanProgress::Hashing { done, total } => bar
                .get_or_insert_with(|| Progress::start(Some(total)))
                .item(done, "files read"),
        },
    )?;
    if let Some(bar) = &mut bar {
        bar.finish();
    }
    for l in listings.iter().filter(|l| l.truncated) {
        eprintln!(
            "warning: stopped at {cap} files in {}; the comparison is incomplete \
             (raise folder_compare_max_files in settings.toml or set \
             folder_compare_max_files_unlimited = true)",
            l.label
        );
    }
    write_table(&table, format)?;
    let differs = any_difference(&table) || listings.iter().any(|l| l.truncated);
    Ok(if differs {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
