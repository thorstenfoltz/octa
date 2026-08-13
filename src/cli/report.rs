//! `--report OUT.html FILE`: an HTML profiling report, scriptable.
//!
//! The document is self-contained (inline CSS, inline SVG, no JavaScript), so
//! it can be attached to a mail or published as-is. The engine is the same one
//! the GUI dialog and the `create_report` tool use.

use std::path::PathBuf;

use octa::data::report::{ReportOptions, ReportSection, build_report};

pub fn run(
    out: PathBuf,
    path: PathBuf,
    table: Option<String>,
    sample: Option<usize>,
    sections: Option<String>,
) -> anyhow::Result<()> {
    let dt = octa::formats::read_table_auto(
        &path,
        table.as_deref(),
        octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )?;

    let sections = match sections {
        None => ReportSection::ALL.to_vec(),
        Some(list) => {
            let mut picked = Vec::new();
            for name in list.split(',') {
                match ReportSection::parse(name) {
                    Some(s) => picked.push(s),
                    None => anyhow::bail!(
                        "unknown report section {name:?}; valid: stats, distributions, top_values, correlation"
                    ),
                }
            }
            picked
        }
    };

    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Data report".to_string());

    let rows: Vec<usize> = (0..dt.row_count()).collect();
    let html = build_report(
        &dt,
        &rows,
        &ReportOptions {
            sections,
            sample_rows: sample,
            title,
            ..ReportOptions::default()
        },
        &std::sync::atomic::AtomicBool::new(false),
    )?;

    std::fs::write(&out, html)?;
    eprintln!("wrote {}", out.display());
    Ok(())
}
