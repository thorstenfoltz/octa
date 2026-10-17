//! `octa --convert <IN> <OUT>` - read a file in one format, write in another.
//!
//! Format inference: both ends are resolved via the standard
//! `FormatRegistry::reader_for_path`. The output reader must support
//! writing (`supports_write` true) - read-only formats like SAS / RDS /
//! HDF5 / NetCDF can't be a `--convert` target. Errors are surfaced
//! verbatim so the user knows whether to pick a different output extension.
//! `--sort-by` sorts the loaded rows before they are written.

use std::path::PathBuf;

use octa::formats::FormatRegistry;

/// The passphrase in environment variable `var`. The error names the
/// variable, never a value.
pub(crate) fn passphrase_from_env(var: &str) -> anyhow::Result<String> {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => anyhow::bail!("--password-env: environment variable {var} is not set or is empty"),
    }
}

pub fn run(
    input: PathBuf,
    output: PathBuf,
    to: Option<String>,
    write_options: octa::formats::write_options::WriteOptions,
    sort: super::SortArgs,
    password_env: Option<String>,
) -> anyhow::Result<()> {
    let step = sort.step()?;
    // Cheap argument checks first: a bad invocation must not read a large
    // file before it fails.
    let pass = match &password_env {
        Some(var) => {
            if output.as_os_str() == "-" {
                anyhow::bail!("--password-env writes a zip file and cannot write to stdout");
            }
            if output
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                anyhow::bail!(
                    "--password-env: name the file inside the zip (e.g. out.csv); Octa adds .zip"
                );
            }
            Some(passphrase_from_env(var)?)
        }
        None => None,
    };
    let mut table = super::read_table(&input)?;
    if let Some(step) = step {
        step.apply(&mut table)?;
        super::action::note_partial_sort(&table);
    }

    if let Some(pass) = &pass {
        let zip = octa::formats::encrypt::zip_path_for(&output);
        let entry = output
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "data.csv".to_string());
        octa::formats::encrypt::write_table_encrypted(&table, &zip, &entry, pass, &write_options)?;
        eprintln!(
            "wrote {} rows x {} columns to {} (password-protected zip)",
            table.row_count(),
            table.col_count(),
            zip.display()
        );
        return Ok(());
    }

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
