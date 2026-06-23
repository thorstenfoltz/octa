//! MCP tool: `run_sql` - run a DuckDB SQL query against one or more files
//! using the multi-table SQL workspace.
//!
//! The primary source (a file or an open tab) is registered as `data`. The
//! optional fields let callers JOIN across multiple sources (`extra_tables`),
//! browse and query whole DBs without copying rows (`attach`), and write the
//! SELECT result to a file - a plain file (CSV / Parquet / ...) or a table in a
//! DuckDB / SQLite database, by extension (`write_to`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::formats::FormatRegistry;
use octa::sql::{
    AttachKind, QueryKind, SqlWorkspace, TableOrigin, WriteMode, WriteTarget, sanitize_sql_name,
};

use crate::mcp::OctaMcpServer;

use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Run a DuckDB SQL query against a tabular source. The primary source (a file `path` or an \
`open_tab`) is registered as `data`. Use `extra_tables` to register more files for JOINs, \
`attach` to ATTACH whole DuckDB / SQLite files (`alias.schema.tbl`), and `write_to` to persist \
the SELECT result to a file - the extension picks the format: csv / tsv / parquet / json / xlsx \
(plain file, `table` is ignored) or duckdb / sqlite (a table inside the database, named by \
`table`). Give just a filename to write into the user's export folder. `limit` caps response \
rows (0 = unlimited).";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the primary file (registered as `data`). Omit when `open_tab`
    /// is set.
    #[serde(default)]
    pub path: PathBuf,

    /// Register an open GUI tab as `data` instead of a file. Pass the tab's
    /// name, or `@active` for the currently active tab. This is the way to
    /// JOIN the table you have open against a file on disk (via
    /// `extra_tables`).
    #[serde(default)]
    pub open_tab: Option<String>,

    /// SQL query string. The primary source is exposed as `data`.
    pub query: String,

    /// Maximum rows to return. Default is the server's configured limit
    /// (1000 unless changed via Octa's Settings -> MCP). Pass 0 for unlimited.
    /// Slices the *response* - set `unlimited` to also lift the file-loader
    /// cap so the query sees every row.
    #[serde(default)]
    pub limit: Option<usize>,

    /// For multi-table sources, load this specific table as `data`.
    #[serde(default)]
    pub table: Option<String>,

    /// Lift the streaming initial-load cap so the query operates on every
    /// row in every loaded file. Default `false`.
    #[serde(default)]
    pub unlimited: bool,

    /// Additional files to register into the workspace before the query runs.
    /// Each entry loads a file and exposes it under the chosen SQL name so the
    /// query can JOIN it against `data`. The SQL name is sanitised.
    #[serde(default)]
    pub extra_tables: Vec<ExtraTable>,

    /// Databases to ATTACH for the duration of the call. After attachment
    /// every inner table is queryable as `alias.schema.tbl` (DuckDB) or
    /// `alias.tbl` (SQLite).
    #[serde(default)]
    pub attach: Vec<AttachSpec>,

    /// When set, write the SELECT result to a file instead of returning rows -
    /// a plain file (csv / tsv / parquet / json / xlsx) or a table inside a
    /// DuckDB / SQLite database, picked by the path extension. The response
    /// shape becomes `{ "kind": "write_back", "rows_written": N, "target": "..." }`.
    #[serde(default)]
    pub write_to: Option<WriteSpec>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExtraTable {
    /// SQL identifier to register the file under (e.g. `customers`).
    pub name: String,
    /// Path to the file to read via the format registry.
    pub path: PathBuf,
    /// Inner-table picker for multi-table sources. Defaults to the reader's
    /// `read_file` behaviour.
    #[serde(default)]
    pub table: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AttachSpec {
    /// SQL alias to attach the database under (e.g. `analytics`).
    pub alias: String,
    /// Path to the database file. The extension picks DuckDB vs. SQLite
    /// (`.duckdb` / `.ddb` -> DuckDB; everything else -> SQLite).
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteSpec {
    /// Output path; the extension picks the format. csv / tsv / parquet / json /
    /// xlsx write a plain file (the SELECT result); duckdb / sqlite write a
    /// table inside the database. A bare filename goes to the export folder.
    pub path: PathBuf,
    /// Target schema (DuckDB only). `null` writes to `main`.
    #[serde(default)]
    pub schema: Option<String>,
    /// Target table name (DuckDB / SQLite targets only; ignored for plain
    /// files like CSV).
    #[serde(default)]
    pub table: String,
    /// Write mode: `create` (default), `replace`, or `append`.
    #[serde(default = "default_write_mode")]
    pub mode: String,
    /// Create the target schema if it doesn't already exist (DuckDB only).
    #[serde(default)]
    pub create_schema_if_missing: bool,
}

fn default_write_mode() -> String {
    "create".to_string()
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    let active = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;
    let mut ws = SqlWorkspace::new()?;
    ws.set_active_table(&active)?;

    for entry in &p.extra_tables {
        let sql_name = sanitize_sql_name(&entry.name);
        // An extra table may itself be an open tab (addressed by handle / name /
        // file name) - register its in-memory snapshot so the model can JOIN any
        // number of open tabs. Otherwise read the file from disk (sandboxed).
        if entry.table.is_none()
            && let Some(snap) = ctx.snapshot_for_pathish(&entry.path.to_string_lossy())
        {
            ws.add_table(
                &sql_name,
                &snap.table,
                TableOrigin::TabClone(snap.display_name.clone()),
            )?;
        } else {
            ctx.ensure_readable(&entry.path)?;
            ws.add_table_from_file(&entry.path, entry.table.as_deref(), &sql_name)?;
        }
    }
    for entry in &p.attach {
        ctx.ensure_readable(&entry.path)?;
        let kind = AttachKind::from_path(&entry.path);
        ws.attach(&entry.path, &entry.alias, kind)?;
    }

    if let Some(spec) = &p.write_to {
        let target_path = ctx.resolve_write_path(&spec.path)?;
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let is_db = matches!(ext.as_str(), "duckdb" | "ddb" | "db" | "sqlite" | "sqlite3");

        if is_db {
            let mode = WriteMode::parse(&spec.mode)?;
            let report = ws.write_result_to_db(&WriteTarget {
                kind: AttachKind::from_path(&target_path),
                path: target_path,
                schema: spec.schema.clone(),
                table: spec.table.clone(),
                mode,
                source_query: p.query.clone(),
                create_schema_if_missing: spec.create_schema_if_missing,
            })?;
            let mut out = Map::new();
            out.insert("kind".to_string(), Value::String("write_back".to_string()));
            out.insert("rows_written".to_string(), Value::from(report.rows_written));
            out.insert(
                "created_schema".to_string(),
                Value::Bool(report.created_schema),
            );
            out.insert(
                "target".to_string(),
                Value::String(report.target_display.clone()),
            );
            return Ok(Value::Object(out));
        }

        // File target (csv, tsv, parquet, json, xlsx, ...): run the SELECT and
        // write the result table via the format registry (overwrites the file).
        let qo = ws.execute(&p.query)?;
        let registry = FormatRegistry::new();
        let out_reader = registry.reader_for_path(&target_path).ok_or_else(|| {
            anyhow::anyhow!(
                "no writer available for the output extension on {}",
                target_path.display()
            )
        })?;
        if !out_reader.supports_write() {
            anyhow::bail!(
                "format {} does not support writing - pick a different output extension",
                out_reader.name()
            );
        }
        if ctx.backup_before_modify && target_path.exists() {
            octa::formats::backup_existing_file(&target_path)?;
        }
        out_reader.write_file_schema_aware(&target_path, &qo.table, ctx.allow_schema_changes)?;
        let mut out = Map::new();
        out.insert("kind".to_string(), Value::String("write_back".to_string()));
        out.insert(
            "rows_written".to_string(),
            Value::from(qo.table.row_count()),
        );
        out.insert(
            "target".to_string(),
            Value::String(target_path.display().to_string()),
        );
        return Ok(Value::Object(out));
    }

    let qo = ws.execute(&p.query)?;
    let kind_str = match qo.kind {
        QueryKind::Select => "select",
        QueryKind::Mutation => "mutation",
    };
    let table_value = table_to_json(&qo.table, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap);
    let mut out = Map::new();
    out.insert("kind".to_string(), Value::String(kind_str.to_string()));
    if let Some(n) = qo.affected {
        out.insert("affected".to_string(), Value::from(n));
    }
    out.insert("result".to_string(), table_value);
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("run_sql failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
