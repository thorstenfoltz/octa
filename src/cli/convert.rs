//! `octa --convert <IN> <OUT>` - read a file in one format, write in another.
//!
//! Format inference: both ends are resolved via the standard
//! `FormatRegistry::reader_for_path`. The output reader must support
//! writing (`supports_write` true) - read-only formats like SAS / RDS /
//! HDF5 / NetCDF can't be a `--convert` target. Errors are surfaced
//! verbatim so the user knows whether to pick a different output extension.

use std::path::PathBuf;

use octa::formats::FormatRegistry;

pub fn run(
    input: PathBuf,
    output: PathBuf,
    to: Option<String>,
    write_options: octa::formats::write_options::WriteOptions,
) -> anyhow::Result<()> {
    let table = super::read_table(&input)?;

    // `-` means stdout. There is no file name to infer a format from, so
    // --to is required; the result is written to a temp file with that
    // extension and copied out, since the writers need a seekable file.
    if output.as_os_str() == "-" {
        let ext = to.ok_or_else(|| {
            anyhow::anyhow!(
                "--convert to `-` needs --to EXT: nothing in a pipe says which \
                 format to write"
            )
        })?;
        let ext = ext.trim_start_matches('.').to_string();
        let registry = FormatRegistry::new();
        let tmp = tempfile::Builder::new()
            .prefix("octa-stdout-")
            .suffix(&format!(".{ext}"))
            .tempfile()?;
        let out_reader = registry
            .reader_for_path(tmp.path())
            .ok_or_else(|| anyhow::anyhow!("no writer available for extension \".{ext}\""))?;
        if !out_reader.supports_write() {
            anyhow::bail!(
                "format {} does not support writing - pick a different --to",
                out_reader.name()
            );
        }
        out_reader.write_file_with_options(tmp.path(), &table, &write_options)?;
        let mut file = std::fs::File::open(tmp.path())?;
        std::io::copy(&mut file, &mut std::io::stdout().lock())?;
        // Counts on stderr so the piped bytes stay exactly the file.
        eprintln!(
            "wrote {} rows x {} columns to stdout",
            table.row_count(),
            table.col_count()
        );
        return Ok(());
    }

    let registry = FormatRegistry::new();
    let out_reader = registry.reader_for_path(&output).ok_or_else(|| {
        anyhow::anyhow!(
            "no reader available for output extension on {}",
            output.display()
        )
    })?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different output extension",
            out_reader.name()
        );
    }
    out_reader.write_file_with_options(&output, &table, &write_options)?;
    eprintln!(
        "wrote {} rows × {} columns to {}",
        table.row_count(),
        table.col_count(),
        output.display()
    );
    Ok(())
}
