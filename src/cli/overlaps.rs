//! `octa --overlaps FILE [--overlaps-start COL] [--overlaps-end COL]
//! [--overlaps-lane COL] [--overlaps-label COL]` - rows whose time spans
//! overlap inside a lane (two bookings of one room, one person on two
//! shifts).
//!
//! **Exits 1 when any overlap is found**, so a script can gate on it; the
//! pairs go to stdout (the same table the GUI's Timeline view opens), the
//! summary to stderr. Without `--overlaps-start` / `--overlaps-end` the
//! first two date columns are used, as the Timeline view does.

use std::path::PathBuf;
use std::process::ExitCode;

use octa::data::timeline::{
    build, column_named, detect, lanes_with_overlaps, overlaps, overlaps_table,
};

use super::OutputFormat;
use super::output::write_table;

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
    pub start: Option<String>,
    pub end: Option<String>,
    pub lane: Option<String>,
    pub label: Option<String>,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<ExitCode> {
    let table = super::read_table(&args.path)?;
    let col = |name: &Option<String>| name.as_deref().map(|n| column_named(&table, n)).transpose();
    let (start, end) = match (col(&args.start)?, col(&args.end)?) {
        (Some(s), e) => (s, e),
        (None, e) => {
            let (s, detected_end) = detect(&table).ok_or_else(|| {
                anyhow::anyhow!("no date column found; name one with --overlaps-start")
            })?;
            (s, e.or(detected_end))
        }
    };
    let rows: Vec<usize> = (0..table.row_count()).collect();
    let tl = build(
        &table,
        &rows,
        start,
        end,
        col(&args.label)?,
        col(&args.lane)?,
    );
    let pairs = overlaps(&tl);

    write_table(&overlaps_table(&tl, &pairs), format)?;
    eprintln!(
        "{} overlap(s) in {} lane(s); start column `{}`, end column `{}`",
        pairs.len(),
        lanes_with_overlaps(&pairs, &tl),
        table.columns[start].name,
        end.map(|e| table.columns[e].name.as_str())
            .unwrap_or("(none)")
    );
    if !tl.bad_rows.is_empty() {
        eprintln!(
            "note: {} row(s) end before they start and were left out: {}",
            tl.bad_rows.len(),
            tl.bad_rows
                .iter()
                .take(20)
                .map(|r| (r + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(if pairs.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
