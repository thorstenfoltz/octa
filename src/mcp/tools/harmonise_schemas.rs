//! MCP tool: `harmonise_schemas` - rewrite a folder of files to one schema.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mcp::OctaMcpServer;

use super::ToolContext;

pub const DESCRIPTION: &str = "Rewrite every data file in a folder to one common set of columns, writing harmonised \
copies into a separate folder. The originals are NEVER modified. The target schema is the shape \
most files already have, or the schema of `target_file` when given. Columns missing from a file \
are added as nulls; columns not in the target are dropped and listed per file. A file whose \
values cannot be cast to a target type is REFUSED rather than written with nulls in place of \
those values, so a harmonised folder never contains silently emptied cells. Returns a per-file \
report plus `written` and `refused` counts. Write tool: creates files.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Folder to scan.
    pub dir: PathBuf,

    /// Folder to write harmonised copies into. Must differ from `dir`.
    pub out_dir: PathBuf,

    /// Take the target schema from this file instead of the majority shape.
    #[serde(default)]
    pub target_file: Option<PathBuf>,

    /// Walk subfolders. Default false.
    #[serde(default)]
    pub recursive: Option<bool>,

    /// Treat column names differing only in case as one column. Default false.
    #[serde(default)]
    pub ignore_case: Option<bool>,

    /// Replace files that already exist in `out_dir`. Default false.
    #[serde(default)]
    pub overwrite: Option<bool>,

    /// Fold the folder into ONE file instead of writing a harmonised copy of
    /// each, adding a `source_file` column saying where every row came from.
    /// `out_dir` then names that single output FILE. Default false.
    #[serde(default)]
    pub combine: Option<bool>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!("harmonise_schemas writes files and this session is read-only");
    }
    let out_dir = ctx.resolve_write_path(&p.out_dir)?;
    if !p.dir.is_dir() {
        anyhow::bail!("{} is not a directory", p.dir.display());
    }
    if p.combine.unwrap_or(false) {
        return run_combine(
            &p.dir,
            &out_dir,
            p.recursive.unwrap_or(false),
            p.ignore_case.unwrap_or(false),
        );
    }
    // The whole safety story rests on the originals being untouched, so this
    // is a refusal rather than a warning.
    if out_dir == p.dir {
        anyhow::bail!("out_dir must differ from dir: harmonised copies never overwrite originals");
    }

    let recursive = p.recursive.unwrap_or(false);
    let ignore_case = p.ignore_case.unwrap_or(false);
    let registry = octa::formats::FormatRegistry::new();
    let (files, skipped) = octa::data::schema_drift::collect_schemas(&p.dir, recursive, &registry);
    if files.is_empty() {
        anyhow::bail!("no readable files in {}", p.dir.display());
    }

    let target = match &p.target_file {
        Some(t) => {
            octa::formats::read_table_auto(
                t,
                None,
                octa::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
            )?
            .columns
        }
        None => octa::data::schema_drift::analyse(
            &files,
            &octa::data::schema_drift::DriftOptions {
                ignore_case,
                recursive,
            },
        )
        .variants
        .first()
        .map(|v| v.columns.clone())
        .unwrap_or_default(),
    };

    let opts = octa::data::harmonise::HarmoniseOptions {
        root: p.dir.clone(),
        out_dir,
        ignore_case,
        overwrite: p.overwrite.unwrap_or(false),
    };
    let plan = octa::data::harmonise::plan_harmonise(&files, &target, &opts);
    let report = octa::data::harmonise::run_harmonise(
        &plan,
        &opts,
        &|_, _| {},
        &std::sync::atomic::AtomicBool::new(false),
        &Default::default(),
    );

    let items: Vec<Value> = report
        .items
        .iter()
        .map(|i| {
            let (status, reason) = match &i.status {
                octa::data::harmonise::FileStatus::AlreadyMatches => ("already_matches", None),
                octa::data::harmonise::FileStatus::Harmonise => ("harmonised", None),
                octa::data::harmonise::FileStatus::Refused(w) => ("refused", Some(w.clone())),
            };
            json!({
                "input": i.input.display().to_string(),
                "output": i.output.display().to_string(),
                "status": status,
                "rows": i.rows,
                "dropped_columns": i.dropped,
                "reason": reason,
            })
        })
        .collect();

    Ok(json!({
        "target_columns": target
            .iter()
            .map(|c| json!({ "name": c.name, "type": c.data_type }))
            .collect::<Vec<_>>(),
        "items": items,
        "written": report.written,
        "refused": report.refused,
        "skipped": skipped
            .iter()
            .map(|(f, why)| json!({ "file": f, "reason": why }))
            .collect::<Vec<_>>(),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("harmonise_schemas failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

/// `combine: true`: fold the folder into one table with a provenance column
/// and write it to `out_dir`, which names a FILE in this mode.
///
/// Shares `normalise_folder::combine_folder` with the CLI and the GUI, so an
/// agent, a script and a person all get the same union schema from the same
/// folder.
fn run_combine(
    dir: &std::path::Path,
    out: &std::path::Path,
    recursive: bool,
    ignore_case: bool,
) -> anyhow::Result<Value> {
    use octa::data::normalise_folder::{CombineOptions, DEFAULT_SOURCE_COLUMN, combine_folder};

    if out.is_dir() {
        anyhow::bail!(
            "with combine, out_dir names the output FILE, but {} is a directory",
            out.display()
        );
    }
    let opts = CombineOptions {
        root: dir.to_path_buf(),
        recursive,
        ignore_case,
        source_column: DEFAULT_SOURCE_COLUMN.to_string(),
    };
    let report = combine_folder(
        &opts,
        &|_, _| {},
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    let registry = octa::formats::FormatRegistry::new();
    let writer = registry
        .reader_for_path(out)
        .ok_or_else(|| anyhow::anyhow!("no writer for {}", out.display()))?;
    writer.write_file_with_options(
        out,
        &report.table,
        &octa::formats::write_options::WriteOptions::default(),
    )?;
    Ok(serde_json::json!({
        "combined": true,
        "out": out.display().to_string(),
        "files_read": report.files_read,
        "rows": report.table.row_count(),
        "columns": report.table.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "skipped": report.skipped.iter().map(|(f, r)| serde_json::json!({"file": f, "reason": r})).collect::<Vec<_>>(),
    }))
}
