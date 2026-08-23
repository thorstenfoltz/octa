//! `--relationships DIR`: which tables in this folder are linked, and how?
//!
//! A report, never a gate, so it always exits 0. The ranking and the orphan
//! counts come from `octa::data::rel_map`, the same engine behind the
//! Relationship map dialog and `suggest_join_keys`, so the three cannot
//! disagree. Unreadable files are noted on stderr rather than aborting the
//! scan, matching `--schema-drift`.

use std::path::PathBuf;

use octa::data::join_keys::DEFAULT_SAMPLE_ROWS;
use octa::data::rel_map::{DEFAULT_MAX_FILES, RelMapOptions, build_map, collect_tables};
use octa::data::summary::num_cell;
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::OutputFormat;
use super::output::write_table;

pub fn run(dir: PathBuf, recursive: bool, format: OutputFormat) -> anyhow::Result<()> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }
    let never_cancel = || false;
    let found = collect_tables(
        &dir,
        recursive,
        DEFAULT_MAX_FILES,
        DEFAULT_SAMPLE_ROWS,
        &never_cancel,
    );
    if found.tables.len() < 2 {
        anyhow::bail!(
            "{} has fewer than two readable tables, so there is nothing to relate",
            dir.display()
        );
    }

    let named: Vec<(String, &DataTable)> =
        found.tables.iter().map(|(n, t)| (n.clone(), t)).collect();
    let map = build_map(&named, &RelMapOptions::default());

    let mut report = DataTable::empty();
    report.columns = [
        ("left_table", "Utf8"),
        ("left_column", "Utf8"),
        ("right_table", "Utf8"),
        ("right_column", "Utf8"),
        ("score", "Float64"),
        ("overlap", "Float64"),
        // Both directions: the count only breaks a tie when read from the
        // child side, and nothing here knows which side that is.
        ("left_orphans", "Int64"),
        ("left_values", "Int64"),
        ("right_orphans", "Int64"),
        ("right_values", "Int64"),
    ]
    .iter()
    .map(|(n, ty)| ColumnInfo {
        name: (*n).to_string(),
        data_type: (*ty).to_string(),
    })
    .collect();
    report.rows = map
        .edges
        .iter()
        .map(|e| {
            vec![
                CellValue::String(map.nodes[e.left_table].name.clone()),
                CellValue::String(map.nodes[e.left_table].columns[e.left_col].clone()),
                CellValue::String(map.nodes[e.right_table].name.clone()),
                CellValue::String(map.nodes[e.right_table].columns[e.right_col].clone()),
                num_cell(e.score),
                num_cell(e.overlap),
                CellValue::Int(e.left_orphans as i64),
                CellValue::Int(e.left_distinct_values as i64),
                CellValue::Int(e.right_orphans as i64),
                CellValue::Int(e.right_distinct_values as i64),
            ]
        })
        .collect();
    write_table(&report, format)?;

    eprintln!("{} table(s) scanned", map.nodes.len());
    if found.truncated {
        eprintln!("stopped after {DEFAULT_MAX_FILES} files");
    }
    for (file, reason) in &found.skipped {
        eprintln!("skipped {file}: {reason}");
    }
    if map.edges.is_empty() {
        eprintln!("no likely relationships found");
    }
    Ok(())
}
