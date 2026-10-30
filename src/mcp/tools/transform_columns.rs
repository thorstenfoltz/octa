//! MCP tool: `transform_columns` - rename, cast, or drop columns of a file and
//! write the result back. This is the one column-level edit `edit_table`
//! deliberately does not do (it only changes cells/rows). Operations apply in a
//! fixed order: **drop**, then **rename**, then **cast** (so cast/rename refer
//! to the post-drop column set, and cast uses the new names).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::CellValue;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, cell_from_json, columns_json, read_with_registry};

pub const DESCRIPTION: &str = "Rename, cast, or drop columns of a tabular file and write the \
result back (the column-level edit `edit_table` does not do). `rename` is a list of \
`{from, to}`; `cast` is a list of `{column, type}` (Arrow type name, e.g. Int64/Float64/Utf8/\
Boolean/Date32/Time64(Microsecond)/Duration(Microsecond), or the words time/duration) that re-types the column and converts its cells; `drop` is a list of column \
names. Operations apply in order: drop, then rename, then cast. Writes to `output_path` (default: \
overwrite `path`); the output format follows its extension. Database files (SQLite/DuckDB/\
GeoPackage) are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameSpec {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CastSpec {
    pub column: String,
    /// Arrow type name (e.g. `Int64`, `Float64`, `Utf8`, `Boolean`, `Date32`,
    /// `Time64(Microsecond)`, `Duration(Microsecond)`), or `time` / `duration`.
    pub r#type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,

    /// Reserved: editing an open GUI tab's columns is not supported. Setting
    /// this returns an error - operate on a file path instead.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,

    /// Columns to drop, by name (applied first).
    #[serde(default)]
    pub drop: Vec<String>,

    /// Columns to rename (`from` -> `to`), applied after drops.
    #[serde(default)]
    pub rename: Vec<RenameSpec>,

    /// Columns to re-type and convert, applied last (using post-rename names).
    #[serde(default)]
    pub cast: Vec<CastSpec>,

    /// Lift the streaming initial-load cap so every row is read and rewritten.
    #[serde(default)]
    pub unlimited: bool,
}

/// Convert one cell to a target Arrow type, reusing `cell_from_json`'s typing
/// rules. Numeric targets parse string cells; unparseable values stay as text.
fn cast_cell(cell: &CellValue, target: &str) -> CellValue {
    if matches!(cell, CellValue::Null) {
        return CellValue::Null;
    }
    // A time type reads the cell's text in any spelling Octa knows.
    if let Some(kind) = octa::data::times::TimeKind::of_type(target) {
        let text = cell.to_string();
        return octa::data::times::text_to_cell(kind, &text).unwrap_or(CellValue::String(text));
    }
    let json = match cell {
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Int(i) => Value::from(*i),
        CellValue::Float(f) => Value::from(*f),
        CellValue::Binary(_) => Value::String(cell.to_string()),
        other => {
            let s = other.to_string();
            let trimmed = s.trim();
            if target.starts_with("Int") || target.starts_with("UInt") {
                trimmed
                    .parse::<i64>()
                    .map(Value::from)
                    .unwrap_or(Value::String(s))
            } else if target.starts_with("Float") || target.contains("Double") {
                trimmed
                    .parse::<f64>()
                    .map(Value::from)
                    .unwrap_or(Value::String(s))
            } else if target == "Boolean" {
                match trimmed.to_ascii_lowercase().as_str() {
                    "true" | "1" => Value::Bool(true),
                    "false" | "0" => Value::Bool(false),
                    _ => Value::String(s),
                }
            } else {
                Value::String(s)
            }
        }
    };
    cell_from_json(&json, target)
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.open_tab.is_some() {
        anyhow::bail!("editing an open tab's columns is not supported; operate on a file path");
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }

    // 1. Drop (highest index first so earlier indices stay valid).
    let mut drop_idx: Vec<usize> = Vec::new();
    for name in &p.drop {
        let idx = table
            .columns
            .iter()
            .position(|c| &c.name == name)
            .ok_or_else(|| anyhow::anyhow!("drop: no such column `{name}`"))?;
        drop_idx.push(idx);
    }
    drop_idx.sort_unstable();
    drop_idx.dedup();
    for idx in drop_idx.into_iter().rev() {
        table.columns.remove(idx);
        for row in &mut table.rows {
            if idx < row.len() {
                row.remove(idx);
            }
        }
    }

    // 2. Rename.
    for r in &p.rename {
        let idx = table
            .columns
            .iter()
            .position(|c| c.name == r.from)
            .ok_or_else(|| anyhow::anyhow!("rename: no such column `{}`", r.from))?;
        table.columns[idx].name = r.to.clone();
    }

    // 3. Cast.
    for c in &p.cast {
        let idx = table
            .columns
            .iter()
            .position(|col| col.name == c.column)
            .ok_or_else(|| anyhow::anyhow!("cast: no such column `{}`", c.column))?;
        let target = octa::data::times::type_from_name(&c.r#type)
            .map_or_else(|| c.r#type.clone(), String::from);
        for row in &mut table.rows {
            if idx < row.len() {
                row[idx] = cast_cell(&row[idx], &target);
            }
        }
        table.columns[idx].data_type = target;
    }
    // The edits overlay no longer matches the rewritten columns; clear it.
    table.edits.clear();

    // Resolve + validate the output target.
    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    let target = ctx.write_transformed(&requested, &table)?;

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert("cols_written".to_string(), Value::from(table.col_count()));
    out.insert("columns".to_string(), columns_json(&table.columns));
    out.insert("output".to_string(), Value::String(target));
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("transform_columns failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
