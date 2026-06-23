//! `octa --partition-by COL --out-dir DIR [--partition-format EXT] FILE`
//!
//! Splits FILE into one output file per distinct value of COL, written into
//! DIR. The output extension defaults to the source file's extension; override
//! with `--partition-format`. Filenames are sanitised via `sanitize_sql_name`
//! so they are safe on all platforms; collisions (which should be rare since
//! values are distinct before sanitising) get a numeric suffix `_2`, `_3`, ...

use std::collections::HashMap;
use std::path::PathBuf;

use octa::data::partition::partition_table;
use octa::formats::FormatRegistry;
use octa::sql::sanitize_sql_name;

pub fn run(
    path: PathBuf,
    col_name: String,
    out_dir: PathBuf,
    partition_format: Option<String>,
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
            "could not create output directory {}: {e}",
            out_dir.display()
        )
    })?;

    // Load the source table.
    let table = super::read_table(&path)?;

    // Resolve the partition column.
    let col_idx = table
        .columns
        .iter()
        .position(|c| c.name == col_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "column \"{col_name}\" not found; available columns: {}",
                table
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    // Verify the output format is writable before we start writing files.
    let dummy_path = PathBuf::from(format!("_check_.{ext}"));
    let registry = FormatRegistry::new();
    let out_reader = registry
        .reader_for_path(&dummy_path)
        .ok_or_else(|| anyhow::anyhow!("no writer available for extension \".{ext}\""))?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different extension",
            out_reader.name()
        );
    }

    // Split the table.
    let groups = partition_table(&table, col_idx);

    // Write each group, deduplicating sanitised stems.
    let mut stem_counts: HashMap<String, usize> = HashMap::new();
    let mut written: Vec<(PathBuf, usize)> = Vec::with_capacity(groups.len());

    for (value, group_table) in &groups {
        let base_stem = sanitize_sql_name(value);
        let count = stem_counts.entry(base_stem.clone()).or_insert(0);
        *count += 1;
        let stem = if *count == 1 {
            base_stem
        } else {
            format!("{base_stem}_{count}")
        };
        let out_path = out_dir.join(format!("{stem}.{ext}"));
        out_reader.write_file(&out_path, group_table)?;
        written.push((out_path, group_table.row_count()));
    }

    // Per-file summary to stdout (parseable: path TAB rows).
    for (p, rows) in &written {
        println!("{}\t{rows}", p.display());
    }

    eprintln!("{} file(s) written to {}", written.len(), out_dir.display());
    Ok(())
}
