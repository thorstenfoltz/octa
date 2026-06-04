//! MCP tool: `export_schema` - render a file's column schema as SQL DDL
//! or a model / interface / struct in another language.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::schema_export::SchemaTarget;

use crate::mcp::OctaMcpServer;

use super::{Source, ToolContext, source_from};

pub const DESCRIPTION: &str = "Generate a schema artifact from a tabular file or open tab: SQL DDL (postgres, mysql, \
sqlite, databricks, snowflake) or a Pydantic v2 model, TypeScript interface, JSON Schema, or \
Rust struct. Pick the output with `target`. Returns `target`, `table_name`, `column_count`, and \
`code`.";

/// Output target. Mirrors `octa::data::schema_export::SchemaTarget`; kept
/// as a separate enum so the library type stays free of a `schemars`
/// derive. Serde renders the variants kebab-case (`json-schema`, ...).
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    Postgres,
    Mysql,
    Sqlite,
    Databricks,
    Snowflake,
    Pydantic,
    Typescript,
    JsonSchema,
    Rust,
}

impl Target {
    fn to_schema_target(self) -> SchemaTarget {
        match self {
            Self::Postgres => SchemaTarget::PostgresSqlDdl,
            Self::Mysql => SchemaTarget::MysqlSqlDdl,
            Self::Sqlite => SchemaTarget::SqliteSqlDdl,
            Self::Databricks => SchemaTarget::DatabricksSqlDdl,
            Self::Snowflake => SchemaTarget::SnowflakeSqlDdl,
            Self::Pydantic => SchemaTarget::PydanticV2,
            Self::Typescript => SchemaTarget::TypeScript,
            Self::JsonSchema => SchemaTarget::JsonSchema,
            Self::Rust => SchemaTarget::RustStruct,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the file whose schema to export. Omit when `open_tab` is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Operate on an open GUI tab instead of a file. Pass the tab's name, or
    /// `@active` for the currently active tab.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage), the table to
    /// inspect. Omit for single-table formats.
    #[serde(default)]
    pub table: Option<String>,

    /// Output target: a SQL DDL dialect (`postgres`, `mysql`, `sqlite`,
    /// `databricks`, `snowflake`) or a language target (`pydantic`,
    /// `typescript`, `json-schema`, `rust`).
    pub target: Target,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let source = source_from(&p.open_tab, &p.path, &p.table);
    // The file stem (or tab name) names the table / class / struct; the
    // renderer sanitises it further.
    let table_name = source_name(ctx, &source);
    let dt = ctx.resolve(&source)?;
    let target = p.target.to_schema_target();
    let code = target.export(&dt.columns, &table_name);

    let mut out = Map::new();
    out.insert(
        "target".to_string(),
        Value::String(target.label().to_string()),
    );
    out.insert("table_name".to_string(), Value::String(table_name));
    out.insert("column_count".to_string(), Value::from(dt.col_count()));
    out.insert("code".to_string(), Value::String(code));
    Ok(Value::Object(out))
}

/// Derive a name for the exported artifact from the source: the file stem for
/// a path, the active/named tab title for a tab, falling back to `data`.
fn source_name(ctx: &ToolContext, source: &Source) -> String {
    let raw = match source {
        Source::Path { path, .. } => path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        Source::ActiveTab => ctx
            .active_tab
            .and_then(|i| ctx.open_tabs.get(i))
            .map(|t| t.display_name.clone())
            .unwrap_or_default(),
        Source::OpenTab(name) => name.clone(),
    };
    // Drop an extension if the tab title carried one (e.g. "sales.csv").
    let stem = std::path::Path::new(&raw)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(raw);
    if stem.is_empty() {
        "data".to_string()
    } else {
        stem
    }
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("export_schema failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
