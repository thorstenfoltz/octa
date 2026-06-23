//! MCP tool: `partition_table` - split a table into one file per distinct
//! value of a column, writing each group into a directory.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::partition::partition_table;
use octa::formats::FormatRegistry;
use octa::sql::sanitize_sql_name;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Split a table into one file per distinct value of a column, \
written into a directory. Returns the list of files written. `column` is the column name to \
partition on; `out_dir` is the output directory (created if absent); `format` overrides the \
output extension (defaults to the source file's extension, or is required when the source is \
an open tab without a known path).";

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

    /// Name of the column whose distinct values become the partition keys.
    pub column: String,

    /// Directory to write the output files into. Created if absent.
    pub out_dir: String,

    /// Output file extension (without the leading dot, e.g. `csv`, `parquet`).
    /// Defaults to the source file's extension. Required when the source is an
    /// open tab that has no associated file path.
    #[serde(default)]
    pub format: Option<String>,
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

    // Resolve the source table.
    let table = ctx.resolve(&source)?;

    // Resolve the partition column.
    let col_idx = table
        .columns
        .iter()
        .position(|c| c.name == p.column)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "column \"{}\" not found; available columns: {}",
                p.column,
                table
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    // Create the output directory.
    let out_dir = PathBuf::from(&p.out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        anyhow::anyhow!(
            "could not create output directory {}: {e}",
            out_dir.display()
        )
    })?;

    // Split and write.
    let groups = partition_table(&table, col_idx);

    let mut stem_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut files: Vec<Value> = Vec::with_capacity(groups.len());

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
        if ctx.backup_before_modify && out_path.exists() {
            octa::formats::backup_existing_file(&out_path)?;
        }
        out_reader.write_file_schema_aware(&out_path, group_table, ctx.allow_schema_changes)?;

        let mut entry = Map::new();
        entry.insert("value".to_string(), Value::String(value.clone()));
        entry.insert(
            "path".to_string(),
            Value::String(out_path.display().to_string()),
        );
        entry.insert("rows".to_string(), Value::from(group_table.row_count()));
        files.push(Value::Object(entry));
    }

    let count = files.len();
    let mut out = Map::new();
    out.insert("files".to_string(), Value::Array(files));
    out.insert("count".to_string(), Value::from(count));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("partition_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
