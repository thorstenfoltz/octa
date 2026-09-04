//! [`dispatch`]: run one [`Action`] and turn its result into an exit code.
//!
//! Split out of `cli/mod.rs`, which had grown to 2,376 lines holding the whole
//! clap surface, the action enum, the detection pass and the dispatcher in one
//! file. Code moved unchanged.

use std::process::ExitCode;

use super::*;

/// Run an action. Returns an `ExitCode` so `main` can exit with the right
/// status; failures map to `ExitCode::FAILURE`. The `Action::Mcp` arm is
/// never reached here - `main.rs` peels it off before calling `dispatch`
/// because it needs to spin up a tokio runtime that the rest of the CLI
/// (and the GUI) deliberately avoid initialising.
///
/// `rows_override`, when `Some(n)`, installs an
/// [`InitialLoadRowsGuard`](octa::formats::InitialLoadRowsGuard) that lifts
/// the process-wide initial-load cap to `n` for the lifetime of the
/// handler. The guard is dropped (and the cap restored) before this function
/// returns.
pub fn dispatch(action: Action, format: OutputFormat, rows_override: Option<usize>) -> ExitCode {
    let _rows_guard = rows_override.map(octa::formats::InitialLoadRowsGuard::new);
    // --validate-schema and --batch-convert decide their own exit codes
    // (schema drift, or a run where some items failed), so they are pulled
    // out of the success/failure mapping below.
    if let Action::ValidateSchema {
        path,
        schema_file,
        table,
    } = action
    {
        return match validate_schema::run(path, schema_file, table, format) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::SchemaDrift {
        dir,
        recursive,
        ignore_case,
    } = action
    {
        return match schema_drift::run(
            dir,
            octa::data::schema_drift::DriftOptions {
                ignore_case,
                recursive,
            },
            format,
        ) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    // Orphan rows are a gate, not a report: a build that finds them should
    // fail, the way --validate-schema and --check do.
    if let Action::CheckReferences(args) = action {
        return match referential::run(*args, format) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::Check { path, rules } = action {
        return match check::run(path, rules, format) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::DriftReport {
        path_a,
        path_b,
        fail_on,
    } = action
    {
        return match drift::run(path_a, path_b, fail_on, format) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::Harmonise {
        dir,
        out_dir,
        target_file,
        recursive,
        ignore_case,
        overwrite,
        write_options,
    } = action
    {
        return match harmonise::run(
            harmonise::Args {
                dir,
                out_dir,
                target_file,
                recursive,
                ignore_case,
                overwrite,
            },
            format,
            &write_options,
        ) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::BatchConvert {
        inputs,
        out_dir,
        target_ext,
        overwrite,
        write_options,
    } = action
    {
        return match batch_convert::run(inputs, out_dir, target_ext, overwrite, write_options) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        };
    }
    let result = match action {
        Action::Schema(path) => schema::run(path, format),
        Action::Head { path, n } => head::run(path, n, format),
        Action::Tail { path, n } => tail::run(path, n, format),
        Action::Sample { path, n, seed } => sample::run(path, n, seed, format),
        Action::Convert {
            input,
            output,
            to,
            write_options,
        } => convert::run(input, output, to, write_options),
        Action::Sql {
            path,
            query,
            extras,
            attachments,
            write_target,
            stream,
        } => sql::run(
            sql::Args {
                path,
                query,
                extras,
                attachments,
                write_target,
                stream,
            },
            format,
        ),
        Action::ExportSchema { path, target } => export_schema::run(path, target),
        Action::CompareSchemas {
            path_a,
            path_b,
            table_a,
            table_b,
        } => compare_schemas::run(path_a, path_b, table_a, table_b, format),
        Action::CompareDistributions(args) => distribution_compare::run(*args, format),
        // Handled above: it decides its own exit code.
        Action::CheckReferences(_) => unreachable!("returned early"),
        Action::Diff {
            path_a,
            path_b,
            db_b,
            mode,
            on,
        } => diff::run(path_a, path_b, db_b, mode, on, format),
        Action::Describe {
            path,
            table,
            sample_rows,
            deep,
        } => describe::run(path, table, sample_rows, deep, format),
        Action::UniqueColumns {
            path,
            table,
            max_combo,
        } => unique_columns::run(path, table, max_combo, format),
        Action::Anonymize { spec, file } => anonymize::run(spec, file, format),
        Action::Union {
            files,
            union_file,
            drop,
            cast,
            ignore_case,
        } => union::run(files, union_file, drop, cast, ignore_case, format),
        Action::Join {
            files,
            join_file,
            join_on,
            join_type,
        } => join::run(files, join_file, join_on, join_type, format),
        Action::Dedupe {
            path,
            dedupe_on,
            dedupe_keep,
        } => dedupe::run(path, dedupe_on, dedupe_keep, format),
        Action::Impute { path, specs } => impute::run(path, specs, format),
        Action::Outliers {
            path,
            method,
            cols,
            k,
        } => outliers::run(path, method, cols, k, format),
        Action::DetectPii { path, sample_rows } => pii::run(path, sample_rows, format),
        Action::Resample { path, spec } => timeseries::run_resample(path, spec, format),
        Action::Rolling { path, spec } => timeseries::run_rolling(path, spec, format),
        Action::Partition {
            path,
            col,
            out_dir,
            format: partition_format,
            layout,
        } => partition::run(path, col, out_dir, partition_format, layout),
        Action::DbQuery { conn, sql } => db::run_query(conn, sql, format),
        Action::ToWorkbook { out, inputs } => workbook::run(out, inputs),
        Action::SyncSql {
            path,
            conn,
            table,
            on,
        } => sync_sql::run(path, conn, table, on),
        Action::CloudLs { url, recursive } => cloud::ls(url, recursive, format),
        Action::CloudGet { url, out } => cloud::get(url, out),
        Action::CloudPut { file, url } => cloud::put(file, url),
        Action::CloudTransfer { from, to, move_it } => cloud::transfer(from, to, move_it),
        Action::CloudDelete { url, recursive } => cloud::delete(url, recursive),
        Action::ListConnections => cloud::list_connections(format),
        Action::AddConnection { spec, secret_env } => connections::add(spec, secret_env),
        Action::RemoveConnection(name) => connections::remove(name),
        Action::DbTables { conn, catalog } => db::run_tables(conn, catalog, format),
        Action::DbWrite {
            conn,
            catalog,
            target,
            mode,
            file,
        } => db::run_write(conn, catalog, target, mode, file),
        Action::DbCopy {
            conn,
            catalog,
            source,
            target_conn,
            target,
            target_catalog,
            mode,
        } => db::run_copy(
            conn,
            catalog,
            source,
            target_conn,
            target,
            target_catalog,
            mode,
        ),
        Action::FuzzyJoin(args) => fuzzy_join::run(*args, format),
        Action::Relationships { dir, recursive } => relationships::run(dir, recursive, format),
        Action::Report {
            out,
            path,
            table,
            sample,
            sections,
        } => report::run(out, path, table, sample, sections),
        Action::ValidateSchema { .. } => unreachable!("handled above"),
        Action::SchemaDrift { .. } => unreachable!("handled above"),
        Action::DriftReport { .. } => unreachable!("handled above"),
        Action::Check { .. } => unreachable!("handled above"),
        Action::Harmonise { .. } => unreachable!("handled above"),
        Action::BatchConvert { .. } => unreachable!("handled above"),
        Action::Completions(shell) => completions::run(shell),
        Action::Mcp => {
            eprintln!("error: --mcp must be dispatched from main, not via cli::dispatch");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // `{:#}`, not `{}`: anyhow's plain Display prints only the
            // outermost context, so a failed DB write said "creating the
            // target table" and nothing about why the server refused it.
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Helper used by every reading action: resolve a path to a reader and
/// load the table. Centralises the "no reader available" error message
/// so every action surfaces consistent wording. Transparently decompresses
/// `.gz` / `.zst` inputs.
/// Split a comma-separated column list, trimming each name and dropping
/// empties, so `--value-cols "a, b,"` reads as `["a", "b"]`.
pub(super) fn split_cols(s: &str) -> Vec<String> {
    s.split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// Read one table for a CLI action.
///
/// A cloud URL (`s3://`, `az://`, `gs://`) is downloaded to a temp file first
/// and then read as usual, so every action taking a FILE takes a cloud object
/// too without a flag of its own. The settings load happens inside the branch:
/// a local read must not pay for it.
/// Buffer stdin into a temp file.
///
/// Every reader needs `Seek` (a Parquet footer is at the end, a CSV sniff
/// rewinds), so a pipe cannot be streamed straight through. The temp file is
/// the honest cost of supporting `-`.
fn stdin_to_temp() -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;
    use std::io::{Read, Write};

    let mut buf = Vec::new();
    std::io::stdin()
        .lock()
        .read_to_end(&mut buf)
        .context("reading stdin")?;
    if buf.is_empty() {
        anyhow::bail!("stdin was empty; `-` expects data on the pipe");
    }
    let mut tmp = tempfile::Builder::new()
        .prefix("octa-stdin-")
        .tempfile()
        .context("creating a temp file for stdin")?;
    tmp.write_all(&buf).context("buffering stdin")?;
    let path = tmp.path().to_path_buf();
    let _ = tmp.keep();
    Ok(path)
}

/// Read a table from `-` (stdin), a URL, or a path.
pub(crate) fn read_table(path: &std::path::Path) -> anyhow::Result<octa::data::DataTable> {
    if path.as_os_str() == "-" {
        let tmp = stdin_to_temp()?;
        // A pipe has no name, so the format has to come from the bytes.
        let name = octa::formats::sniff::sniff_format(&tmp).ok_or_else(|| {
            anyhow::anyhow!(
                "cannot tell what format the piped data is; sniffing recognises \
                 Parquet, Arrow, Avro, JSON, JSON Lines, CSV and TSV"
            )
        })?;
        let registry = octa::formats::FormatRegistry::new();
        let reader = registry
            .reader_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("no reader named {name}"))?;
        return reader.read_file(&tmp);
    }
    let as_str = path.to_string_lossy();
    // The plainest URL of all: no credentials, no provider, just a download.
    // Checked before the cloud branch because the schemes are disjoint and
    // this one needs no settings load.
    if octa::cloud::is_http_url(&as_str) {
        let fetched =
            octa::cloud::fetch_http_to_temp(&as_str, octa::cloud::UrlTrust::UserSupplied)?;
        if let Some(final_url) = &fetched.redirected_to {
            // Stderr, so a piped result stays parseable. There is nobody to
            // ask on a command line; the GUI raises a confirmation instead.
            eprintln!("note: {as_str} redirected to {final_url}");
        }
        return octa::formats::read_table_auto(
            &fetched.path,
            None,
            octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
        );
    }
    if octa::cloud::parse_cloud_url(&as_str).is_some() {
        let settings = octa::ui::settings::AppSettings::load();
        let tmp = octa::cloud::fetch_url_to_temp(&as_str, &settings)?;
        return octa::formats::read_table_auto(
            &tmp,
            None,
            octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
        );
    }
    octa::formats::read_table_auto(
        path,
        None,
        octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
    )
}
