//! `octa --partition-by COL | --split-rows N | --split-size SIZE --out-dir DIR FILE`
//!
//! Splits FILE into DIR: one file per distinct value of COL, and/or parts of
//! at most N rows / SIZE bytes. Naming and the size search live in
//! `octa::data::partition::write_split`, shared with the dialog and the
//! `partition_table` tool.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use octa::data::partition::{PartitionLayout, SplitIo, SplitSpec, write_split};
use octa::formats::FormatRegistry;

pub fn run(
    path: PathBuf,
    col_name: Option<String>,
    out_dir: PathBuf,
    partition_format: Option<String>,
    layout: PartitionLayout,
    max_rows: Option<usize>,
    max_bytes: Option<u64>,
) -> anyhow::Result<()> {
    // Determine output extension: --partition-format takes precedence, then
    // the source file's own extension.
    let ext = if let Some(fmt) = &partition_format {
        fmt.trim_start_matches('.').to_string()
    } else {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "cannot determine output format: the source file has no extension; \
                     pass --partition-format EXT to specify one"
                )
            })?
    };

    std::fs::create_dir_all(&out_dir).map_err(|e| {
        anyhow::anyhow!(
            "could not create output directory {}: {e:#}",
            out_dir.display()
        )
    })?;

    let table = super::read_table(&path)?;
    let column = match &col_name {
        Some(name) => Some(
            table
                .columns
                .iter()
                .position(|c| &c.name == name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "column \"{name}\" not found; available columns: {}",
                        table
                            .columns
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?,
        ),
        None => None,
    };
    let registry = FormatRegistry::new();
    let out_reader = registry
        .reader_for_path(&PathBuf::from(format!("_check_.{ext}")))
        .ok_or_else(|| anyhow::anyhow!("no writer available for extension \".{ext}\""))?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different extension",
            out_reader.name()
        );
    }
    let spec = SplitSpec {
        column,
        layout,
        max_rows,
        max_bytes,
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());

    let mut bar = super::progress::Progress::start(None);
    let outcome = write_split(
        &table,
        &spec,
        &out_dir,
        &ext,
        &stem,
        SplitIo {
            write: &mut |t, p| out_reader.write_file(p, t),
            cancel: &AtomicBool::new(false),
            progress: &mut |n, p| bar.item(n, &super::progress::short_name(p)),
        },
    )?;
    bar.finish();

    // Per-file summary to stdout (parseable: path TAB rows).
    for part in &outcome.parts {
        println!("{}\t{}", part.path.display(), part.rows);
    }
    eprintln!(
        "{} file(s) written to {}",
        outcome.parts.len(),
        out_dir.display()
    );
    Ok(())
}
