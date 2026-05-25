//! `octa --convert <IN> <OUT>` — read a file in one format, write in another.
//!
//! Format inference: both ends are resolved via the standard
//! `FormatRegistry::reader_for_path`. The output reader must support
//! writing (`supports_write` true) — read-only formats like SAS / RDS /
//! HDF5 / NetCDF can't be a `--convert` target. Errors are surfaced
//! verbatim so the user knows whether to pick a different output extension.

use std::path::PathBuf;

use octa::formats::FormatRegistry;

pub fn run(input: PathBuf, output: PathBuf) -> anyhow::Result<()> {
    let table = super::read_table(&input)?;
    let registry = FormatRegistry::new();
    let out_reader = registry.reader_for_path(&output).ok_or_else(|| {
        anyhow::anyhow!(
            "no reader available for output extension on {}",
            output.display()
        )
    })?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing — pick a different output extension",
            out_reader.name()
        );
    }
    out_reader.write_file(&output, &table)?;
    eprintln!(
        "wrote {} rows × {} columns to {}",
        table.row_count(),
        table.col_count(),
        output.display()
    );
    Ok(())
}
