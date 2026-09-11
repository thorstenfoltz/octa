//! `octa --sql <FILE> --query '<SQL>'` runs a SQL query against a file
//! and prints the result. Extras and ATTACH-ed databases extend the SQL
//! workspace so JOINs and write-back work the same way they do in the GUI
//! and MCP surfaces.
//!
//! Plain single-file invocations (no `--sql-table`, no `--sql-attach`,
//! no `--sql-write-to`) keep the byte-for-byte behaviour of the original
//! one-shot path: the active file is registered as `data`, the query runs,
//! and the result is rendered through the shared output writer.

use std::path::PathBuf;

use anyhow::Context;
use octa::sql::{AttachKind, QueryKind, SqlWorkspace, WriteMode, WriteTarget, sanitize_sql_name};

use super::NamedPath;
use super::OutputFormat;
use super::output::write_table;

/// Where a successful `--sql --sql-write-to ...` should write its result.
/// Built by `Cli::detect_action`; CLI-only because the library type is
/// re-used by both CLI and MCP surfaces under the same name.
#[derive(Debug, Clone)]
pub struct SqlWriteSpec {
    pub path: PathBuf,
    pub schema: Option<String>,
    pub table: String,
    pub mode: WriteMode,
}

/// Everything `--sql` needs, bundled so the function keeps a readable
/// signature as the flag list grows.
pub struct Args {
    pub path: PathBuf,
    pub query: String,
    pub extras: Vec<NamedPath>,
    pub attachments: Vec<NamedPath>,
    pub write_target: Option<SqlWriteSpec>,
    /// Register the primary file as a scan view instead of loading its rows.
    pub stream: bool,
}

pub fn run(args: Args, format: OutputFormat) -> anyhow::Result<()> {
    let Args {
        path,
        query,
        extras,
        attachments,
        write_target,
        stream,
    } = args;
    // Build the workspace and register the primary file as `data`. The
    // primary file is *always* loaded as `data`; this is the contract the
    // single-file form has carried since the CLI shipped.
    let mut ws = SqlWorkspace::new()?;
    // Streaming registers `data` as a view over the file, so an aggregate can
    // cover every row of a file far larger than memory. Only the formats
    // DuckDB can scan qualify; anything else falls back with a note rather
    // than failing, because the query itself is still perfectly runnable.
    let streamed = stream
        && octa::formats::large::ScanKind::for_path(&path).is_some()
        && ws.add_view_from_scan("data", &path).is_ok();
    if stream && !streamed {
        eprintln!(
            "note: --stream does not apply to {}, reading it normally",
            path.display()
        );
    }
    if !streamed {
        let active = super::read_table(&path)?;
        ws.set_active_table(&active)?;
    }

    for entry in &extras {
        let sql_name = sanitize_sql_name(&entry.name);
        ws.add_table_from_file(&entry.path, None, &sql_name)
            .with_context(|| {
                format!(
                    "registering extra table '{}' from {}",
                    entry.name,
                    entry.path.display()
                )
            })?;
    }

    for entry in &attachments {
        let kind = AttachKind::from_path(&entry.path);
        ws.attach(&entry.path, &entry.name, kind).with_context(|| {
            format!(
                "ATTACHing database '{}' from {}",
                entry.name,
                entry.path.display()
            )
        })?;
    }

    if let Some(target) = write_target {
        let report = ws.write_result_to_db(&WriteTarget {
            path: target.path.clone(),
            kind: AttachKind::from_path(&target.path),
            schema: target.schema.clone(),
            table: target.table.clone(),
            mode: target.mode,
            source_query: query.clone(),
            create_schema_if_missing: true,
        })?;
        eprintln!(
            "wrote {} row(s) to {}",
            report.rows_written, report.target_display
        );
        // Tidy up workspace attachments so the connection releases any
        // file locks on the target / extras before this process exits.
        for entry in &attachments {
            let _ = ws.detach(&entry.name);
        }
        let _ = ws;
        return Ok(());
    }

    let outcome = ws.execute(&query)?;
    match outcome.kind {
        QueryKind::Select => {
            write_table(&outcome.table, format)?;
        }
        QueryKind::Mutation => {
            if let Some(name) = outcome.created.as_ref() {
                eprintln!("created {name}");
            } else if let Some(n) = outcome.affected {
                eprintln!("{n} rows affected");
            } else {
                eprintln!("mutation completed");
            }
            // Surface the post-mutation `data` contents so the user can
            // pipe it on or compare; mutations against an in-memory
            // workspace are not persisted to the source file by `--sql`.
            write_table(&outcome.table, format)?;
        }
    }
    // The workspace owns its own registration of the underlying data; these
    // bindings only existed to build it.
    let _ = extras;
    let _ = attachments;
    Ok(())
}
