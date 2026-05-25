//! MCP tool: `export_schema` — render a file's column schema as SQL DDL
//! or a model / interface / struct in another language.

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::schema_export::SchemaTarget;

use crate::mcp::OctaMcpServer;

use super::read_with_registry;

// Tool description lives inline at the `#[tool]` site in `src/mcp/mod.rs`.

/// Output target. Mirrors `octa::data::schema_export::SchemaTarget`; kept
/// as a separate enum so the library type stays free of a `schemars`
/// derive. Serde renders the variants kebab-case (`json-schema`, …).
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
    /// Path to the file whose schema to export.
    pub path: PathBuf,

    /// For multi-table sources (SQLite, DuckDB, GeoPackage), the table to
    /// inspect. Omit for single-table formats.
    #[serde(default)]
    pub table: Option<String>,

    /// Output target: a SQL DDL dialect (`postgres`, `mysql`, `sqlite`,
    /// `databricks`, `snowflake`) or a language target (`pydantic`,
    /// `typescript`, `json-schema`, `rust`).
    pub target: Target,
}

pub async fn handle(_server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let path = p.path.clone();
    let table_name_opt = p.table.clone();
    let target = p.target.to_schema_target();

    let (columns, table_name) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let dt = read_with_registry(&path, table_name_opt.as_deref())?;
        // Same rule the GUI dialog + CLI use: the file stem names the
        // table / class / struct; the renderer sanitises it further.
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "data".to_string());
        Ok((dt.columns, name))
    })
    .await
    .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
    .map_err(|e| McpError::invalid_params(format!("export_schema failed: {e}"), None))?;

    let code = target.export(&columns, &table_name);

    let mut out = Map::new();
    out.insert(
        "target".to_string(),
        Value::String(target.label().to_string()),
    );
    out.insert("table_name".to_string(), Value::String(table_name));
    out.insert("column_count".to_string(), Value::from(columns.len()));
    out.insert("code".to_string(), Value::String(code));
    Ok(CallToolResult::success(vec![Content::text(
        Value::Object(out).to_string(),
    )]))
}
