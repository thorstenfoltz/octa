//! MCP tool: `write_table` - write model-supplied rows to a file in any
//! writable format. The inverse of `read_table`: rows come in as an
//! array-of-arrays matching `columns`, and the output extension picks the
//! writer (CSV, Parquet, JSON, Excel, ...). Three modes: `create` (refuse to
//! clobber), `overwrite` (replace the whole file), and `append` (read the
//! existing file, validate the schema, and rewrite it with the extra rows).
//!
//! Database targets (`.sqlite` / `.duckdb`) are out of scope - their writers
//! require a table loaded from the database (diff-based save). Use `edit_table`
//! or `run_sql` with `write_to` for those.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, build_data_table, cell_from_json, read_with_registry};

pub const DESCRIPTION: &str = "Write model-supplied rows to a file in any writable format - the inverse of `read_table`. \
The output extension picks the format. Supply `columns` (name + optional Arrow `type`, default \
`Utf8`) and `rows` (array-of-arrays). `mode` is `create` (default), `overwrite`, or `append`. \
Database files are not valid targets - use `edit_table` or `run_sql` with `write_to`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the output file. Extension determines the write format.
    pub path: PathBuf,

    /// Reserved: writing back into an open GUI tab is not supported in v1.
    /// Setting this returns an error - save to a file path instead.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// Column definitions, in order. `type` is an Arrow type name (e.g.
    /// `Int64`, `Float64`, `Boolean`, `Date32`, `Timestamp(Microsecond, None)`,
    /// `Utf8`) and defaults to `Utf8` when omitted.
    pub columns: Vec<ColumnSpec>,

    /// Rows as an array-of-arrays: each inner array is one row whose cells line
    /// up positionally with `columns`. Same shape `read_table` returns, so a
    /// read result round-trips straight back in.
    #[serde(default)]
    pub rows: Vec<Vec<Value>>,

    /// `create` (default): error if the file already exists. `overwrite`:
    /// replace the whole file. `append`: the file must exist; its columns must
    /// match `columns` by name, and the new rows are added to the end.
    #[serde(default = "default_mode")]
    pub mode: String,

    /// Lift the streaming initial-load cap when reading the existing file for
    /// `append`, so every existing row is preserved. Default `false`.
    #[serde(default)]
    pub unlimited: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ColumnSpec {
    /// Column name.
    pub name: String,
    /// Arrow type name. Defaults to `Utf8`.
    #[serde(default)]
    pub r#type: Option<String>,
}

fn default_mode() -> String {
    "create".to_string()
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.open_tab.is_some() {
        anyhow::bail!(
            "writing back to an open tab is not supported yet; save to a file path instead"
        );
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let columns: Vec<(String, String)> = p
        .columns
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                c.r#type.clone().unwrap_or_else(|| "Utf8".to_string()),
            )
        })
        .collect();
    let rows = &p.rows;
    // Confine chat writes to the export dir (no-op for MCP / CLI).
    let path = ctx.resolve_write_path(&p.path)?;
    let path = &path;

    let registry = FormatRegistry::new();
    let out_reader = registry.reader_for_path(path).ok_or_else(|| {
        anyhow::anyhow!(
            "no reader available for output extension on {}",
            path.display()
        )
    })?;
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different output extension",
            out_reader.name()
        );
    }

    let table = match p.mode.as_str() {
        "create" => {
            if path.exists() {
                anyhow::bail!(
                    "{} already exists - use mode \"overwrite\" to replace it or \
                     \"append\" to add rows",
                    path.display()
                );
            }
            build_data_table(&columns, rows)?
        }
        "overwrite" => build_data_table(&columns, rows)?,
        "append" => {
            if !path.exists() {
                anyhow::bail!(
                    "{} does not exist - use mode \"create\" to make it",
                    path.display()
                );
            }
            let mut existing = read_with_registry(path, None)?;
            let existing_names: Vec<&str> =
                existing.columns.iter().map(|c| c.name.as_str()).collect();
            let requested_names: Vec<&str> = columns.iter().map(|(n, _)| n.as_str()).collect();
            if existing_names != requested_names {
                anyhow::bail!(
                    "append column mismatch: file has [{}] but request has [{}]",
                    existing_names.join(", "),
                    requested_names.join(", ")
                );
            }
            for (i, row) in rows.iter().enumerate() {
                if row.len() != existing.columns.len() {
                    anyhow::bail!(
                        "row {i} has {} cell(s) but the table has {} column(s)",
                        row.len(),
                        existing.columns.len()
                    );
                }
                let cells = row
                    .iter()
                    .enumerate()
                    .map(|(c, v)| cell_from_json(v, &existing.columns[c].data_type))
                    .collect();
                existing.rows.push(cells);
            }
            existing
        }
        other => anyhow::bail!(
            "unknown mode \"{other}\" - expected \"create\", \"overwrite\", or \"append\""
        ),
    };

    out_reader.write_file(path, &table)?;

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert("cols_written".to_string(), Value::from(table.col_count()));
    out.insert(
        "output".to_string(),
        Value::String(path.display().to_string()),
    );
    out.insert("mode".to_string(), Value::String(p.mode.clone()));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("write_table failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
