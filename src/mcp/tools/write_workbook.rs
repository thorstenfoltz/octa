//! MCP tool: `write_workbook` - write several tables into one `.xlsx`, one
//! worksheet per entry.
//!
//! A **write** tool: it is in `WRITE_TOOL_NAMES`, removed under
//! `--mcp-read-only`, and bails on a read-only context. The writing is
//! `excel_reader::write_workbook`, shared with the GUI dialog and the CLI's
//! `--to-workbook`, so sheet naming cannot diverge between the three.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::DataTable;
use octa::formats::excel_reader::write_workbook;
use octa::formats::write_options::TableStyle;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Write several tables into ONE .xlsx workbook, one worksheet \
per entry. Each sheet names a source: `path` (any readable file) or `open_tab` (in-GUI \
assistant only), plus an optional sheet `name` (default: the file stem or tab name). Sheet \
names are corrected to Excel's rules automatically - at most 31 characters, no forbidden \
punctuation, duplicates numbered - so a write cannot produce a workbook Excel refuses to \
open. Use this instead of calling `convert` once per table when the user wants one file.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SheetSpec {
    /// Source file for this sheet. Omit when using `open_tab`.
    #[serde(default)]
    pub path: std::path::PathBuf,
    /// Open tab name, `@active`, or a handle such as `#2`, instead of `path`.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// Inner table/sheet name for a multi-table source file.
    #[serde(default)]
    pub table: Option<String>,
    /// Worksheet name. Default: the file stem or the tab name.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Destination `.xlsx` path.
    pub out_path: String,
    /// One entry per worksheet, in workbook order. At least one.
    pub sheets: Vec<SheetSpec>,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if ctx.read_only {
        anyhow::bail!("writes are disabled for this session (read-only server or profile)");
    }
    if p.sheets.is_empty() {
        anyhow::bail!("`sheets` needs at least one entry");
    }

    // Read every source before writing: a workbook half-written because the
    // fifth source was unreadable is worse than no workbook at all.
    let mut tables: Vec<DataTable> = Vec::with_capacity(p.sheets.len());
    let mut names: Vec<String> = Vec::with_capacity(p.sheets.len());
    for spec in &p.sheets {
        let source = source_from(&spec.open_tab, &spec.path, &spec.table);
        tables.push(ctx.resolve(&source)?);
        names.push(match &spec.name {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => spec
                .open_tab
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    spec.path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "Sheet".to_string()),
        });
    }

    let dest = ctx.resolve_write_dest(std::path::Path::new(&p.out_path))?;
    let sheets: Vec<(String, &DataTable, Option<&TableStyle>)> = names
        .iter()
        .zip(tables.iter())
        .map(|(n, t)| (n.clone(), t, None))
        .collect();
    write_workbook(dest.path(), &sheets)?;
    let written = dest.finish()?;

    let mut taken: Vec<String> = Vec::new();
    let sheet_names: Vec<String> = names
        .iter()
        .map(|n| octa::formats::xlsx_style::sanitize_sheet_name(n, &mut taken))
        .collect();
    Ok(json!({
        "path": written,
        "sheets": sheet_names,
        "rows": tables.iter().map(|t| t.row_count()).collect::<Vec<_>>(),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("write_workbook failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
