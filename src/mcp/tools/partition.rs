//! MCP tool: `partition_table` - split a table into files by column value
//! and/or row / size limits, writing them into a directory.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::partition::PartitionLayout;
use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Split a table into files: one file per distinct value of a column, and/or parts of at most max_rows rows or max_bytes bytes (files then get a part number). Pass at least one of column, max_rows, max_bytes. out_dir is created if absent; format overrides the output extension (defaults to the source file's, required for an open tab without a path). Returns { files: [{value, path, rows}], count, partial }; partial is true when the source was cut at the load cap (pass unlimited: true). This is a write tool and is unavailable in read-only mode.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources, the specific table to read.
    #[serde(default)]
    pub table: Option<String>,

    /// Column whose distinct values become the partition keys. Optional when max_rows or max_bytes is set.
    #[serde(default)]
    pub column: Option<String>,

    /// Directory to write the output files into. Created if absent.
    pub out_dir: String,

    /// Output file extension (without the leading dot, e.g. `csv`, `parquet`).
    /// Defaults to the source file's extension. Required when the source is an
    /// open tab that has no associated file path.
    #[serde(default)]
    pub format: Option<String>,

    /// Naming: `flat` (default) writes `<value>.<ext>` side by side;
    /// `folder` writes `<value>/part-0001.<ext>`; `hive` writes
    /// `<column>=<value>/data.<ext>`; `hive-parts` writes
    /// `<column>=<value>/part-0001.<ext>`. All four hold the same rows and all
    /// four reopen as one partitioned table - only the names differ.
    #[serde(default)]
    pub layout: Option<String>,

    /// At most this many rows per file. Files then get a part number.
    #[serde(default)]
    pub max_rows: Option<usize>,

    /// At most this many bytes per file (they land a little under it).
    #[serde(default)]
    pub max_bytes: Option<u64>,

    /// Lift the streaming load cap so every source row is split. Default
    /// `false`; the response's `partial` says when rows were left out.
    #[serde(default)]
    pub unlimited: bool,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);

    // Determine the output extension before resolving the table, so we can
    // surface a clear error early when neither `format` nor a file extension
    // is available.
    let ext = if let Some(fmt) = &p.format {
        fmt.trim_start_matches('.').to_string()
    } else {
        // Try to get the extension from the source path.
        let path_ext = if p.open_tab.is_none() && !p.path.as_os_str().is_empty() {
            p.path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        path_ext.ok_or_else(|| {
            anyhow::anyhow!(
                "cannot determine output format: the source has no file extension; \
                 pass `format` to specify one (e.g. \"csv\" or \"parquet\")"
            )
        })?
    };

    // Verify the format is writable up front.
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

    // Cheap refusal before the table is read; `validate` below stays the real guard.
    if p.column.is_none() && p.max_rows.is_none() && p.max_bytes.is_none() {
        anyhow::bail!(
            "choose a column, a row limit or a size limit to split by (pass `column`, `max_rows` or `max_bytes`)"
        );
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));
    let table = ctx.resolve(&source)?;
    let column = match &p.column {
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
    let layout = match &p.layout {
        Some(word) => PartitionLayout::parse(word).ok_or_else(|| {
            anyhow::anyhow!("`layout` must be one of: flat, folder, hive, hive-parts")
        })?,
        None => PartitionLayout::default(),
    };
    let spec = octa::data::partition::SplitSpec {
        column,
        layout,
        max_rows: p.max_rows,
        max_bytes: p.max_bytes,
    };
    spec.validate()
        .map_err(|e| anyhow::anyhow!("{e:#} (pass `column`, `max_rows` or `max_bytes`)"))?;

    let out_dir = PathBuf::from(&p.out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        anyhow::anyhow!(
            "could not create output directory {}: {e:#}",
            out_dir.display()
        )
    })?;
    let stem = p
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .or_else(|| p.open_tab.clone())
        .unwrap_or_else(|| "data".to_string());
    let outcome = octa::data::partition::write_split(
        &table,
        &spec,
        &out_dir,
        &ext,
        &stem,
        octa::data::partition::SplitIo {
            write: &mut |t, path| {
                if ctx.backup_before_modify && path.exists() {
                    octa::formats::backup_existing_file(path)?;
                }
                out_reader.write_file_schema_aware(
                    path,
                    t,
                    ctx.allow_schema_changes,
                    &Default::default(),
                )
            },
            cancel: &std::sync::atomic::AtomicBool::new(false),
            progress: &mut |_, _| {},
        },
    )?;

    let files: Vec<Value> = outcome
        .parts
        .iter()
        .map(|part| {
            let mut entry = Map::new();
            entry.insert(
                "value".to_string(),
                part.value.clone().map(Value::String).unwrap_or(Value::Null),
            );
            entry.insert(
                "path".to_string(),
                Value::String(part.path.display().to_string()),
            );
            entry.insert("rows".to_string(), Value::from(part.rows));
            Value::Object(entry)
        })
        .collect();
    let mut out = Map::new();
    out.insert("count".to_string(), Value::from(files.len()));
    out.insert("files".to_string(), Value::Array(files));
    out.insert("partial".to_string(), Value::Bool(table.is_partial()));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("partition_table failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
