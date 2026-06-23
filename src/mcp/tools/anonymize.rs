//! MCP tool: `anonymize` - mask / scramble chosen columns of a file and write
//! the sanitised result. A write tool (dropped under `--mcp-read-only`).

use std::path::PathBuf;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::transform::{AnonRule, AnonSource, AnonSpec, AnonStrategy, anonymize_table};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatRegistry;

use crate::mcp::OctaMcpServer;

use super::{ToolContext, read_with_registry};

pub const DESCRIPTION: &str = "Anonymise / mask sensitive columns of a tabular file and write the \
sanitised result. `rules` is a list of `{columns, strategy, new_column?}`; `columns` is one name \
or an array of names (two or more with a hash strategy combine them into one new column named \
`new_column`). `output` is `in_place` (default, overwrite) or `new_columns` (keep originals, \
append). `strategy` is one of: \
`{type:\"hash\", algo:\"sha256\"|\"blake3\", length?:N}` (omit length for the full 64-char \
digest; stable + join-able), \
`{type:\"partial_mask\", keep:\"first\"|\"last\", count:N, mask_char:\"*\"}`, \
`{type:\"redact\", token:{\"fixed\":\"[REDACTED]\"}|\"null\"}`, or `{type:\"fake\", \
kind:\"name\"|\"email\"|\"city\"|\"company\"|\"phone\"|\"uuid\"}`. A shared `salt` makes the \
output non-guessable; the same value+salt always maps the same way (duplicates stay linked). \
Null/empty cells pass through. Writes to `output_path` (default: overwrite `path`); the format \
follows its extension. Database files are not valid sources or targets.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ColumnsField {
    One(String),
    Many(Vec<String>),
}

impl ColumnsField {
    fn names(&self) -> Vec<String> {
        match self {
            ColumnsField::One(s) => vec![s.clone()],
            ColumnsField::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RuleParam {
    /// One column name or an array of names. Two or more names with a hash
    /// strategy combine them into one new column.
    pub columns: ColumnsField,
    /// Name for the combined-hash new column (multi-column hash only).
    #[serde(default)]
    pub new_column: Option<String>,
    /// How to scramble it (see the tool description for the shapes).
    pub strategy: AnonStrategy,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Path to the source file.
    pub path: PathBuf,

    /// Reserved: anonymising an open GUI tab is not supported here. Setting
    /// this returns an error - operate on a file path instead.
    #[serde(default)]
    pub open_tab: Option<String>,

    /// Where to write the result. Defaults to `path` (overwrite in place).
    #[serde(default)]
    pub output_path: Option<PathBuf>,

    /// Per-column rules. Must be non-empty; every `column` must exist.
    pub rules: Vec<RuleParam>,

    /// Shared salt for all rules. Empty = plain deterministic hashing.
    #[serde(default)]
    pub salt: String,

    /// `in_place` (default) overwrites the columns; `new_columns` keeps the
    /// originals and appends the anonymised values as new columns.
    #[serde(default)]
    pub output: Option<String>,

    /// Lift the streaming initial-load cap so every row is read + rewritten.
    #[serde(default)]
    pub unlimited: bool,
}

const DB_FORMATS: &[&str] = &["SQLite", "DuckDB", "GeoPackage"];

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    if p.open_tab.is_some() {
        anyhow::bail!("anonymising an open tab is not supported; operate on a file path");
    }
    if p.rules.is_empty() {
        anyhow::bail!("rules must not be empty");
    }
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    ctx.ensure_readable(&p.path)?;
    let mut table = read_with_registry(&p.path, None)?;
    if table.db_meta.is_some() {
        anyhow::bail!("database files (SQLite/DuckDB/GeoPackage) are not valid sources");
    }

    let new_columns = matches!(p.output.as_deref(), Some("new_columns"));

    // Resolve names to indices.
    let mut rules: Vec<AnonRule> = Vec::with_capacity(p.rules.len());
    for r in &p.rules {
        let mut cols = Vec::new();
        for name in r.columns.names() {
            let idx = table
                .columns
                .iter()
                .position(|c| c.name == name)
                .ok_or_else(|| anyhow::anyhow!("no such column: {name}"))?;
            cols.push(idx);
        }
        rules.push(AnonRule {
            columns: cols,
            strategy: r.strategy.clone(),
            new_column: r.new_column.clone(),
        });
    }
    let spec = AnonSpec {
        rules,
        salt: p.salt.clone(),
    };
    let outputs = anonymize_table(&table, &spec);
    let columns_anonymized = outputs.len();
    for o in outputs {
        match o.source {
            AnonSource::Column(c) if !new_columns => {
                for (row, v) in o.values.into_iter().enumerate() {
                    table.set(row, c, v);
                }
            }
            AnonSource::Column(c) => {
                let base = table
                    .columns
                    .get(c)
                    .map(|x| x.name.clone())
                    .unwrap_or_default();
                append_column(&mut table, &format!("{base}_anon"), o.values);
            }
            AnonSource::Derived { name } => append_column(&mut table, &name, o.values),
        }
    }
    table.apply_edits();

    // Resolve + validate output.
    let requested = p.output_path.clone().unwrap_or_else(|| p.path.clone());
    let out_path = ctx.resolve_write_path(&requested)?;
    let registry = FormatRegistry::new();
    let out_reader = registry.reader_for_path(&out_path).ok_or_else(|| {
        anyhow::anyhow!("no reader for output extension on {}", out_path.display())
    })?;
    if DB_FORMATS.contains(&out_reader.name()) {
        anyhow::bail!(
            "database files ({}) are not valid targets",
            out_reader.name()
        );
    }
    if !out_reader.supports_write() {
        anyhow::bail!(
            "format {} does not support writing - pick a different output extension",
            out_reader.name()
        );
    }
    if ctx.backup_before_modify && out_path.exists() {
        octa::formats::backup_existing_file(&out_path)?;
    }
    out_reader.write_file_schema_aware(&out_path, &table, ctx.allow_schema_changes)?;

    let mut out = Map::new();
    out.insert("rows_written".to_string(), Value::from(table.row_count()));
    out.insert(
        "columns_anonymized".to_string(),
        Value::from(columns_anonymized),
    );
    out.insert(
        "output".to_string(),
        Value::String(out_path.display().to_string()),
    );
    Ok(Value::Object(out))
}

/// Append a new Utf8 column with the given values (uniquifying the name).
fn append_column(table: &mut DataTable, name: &str, values: Vec<CellValue>) {
    let mut unique = name.to_string();
    let mut k = 2;
    while table.columns.iter().any(|c| c.name == unique) {
        unique = format!("{name}_{k}");
        k += 1;
    }
    table.columns.push(ColumnInfo {
        name: unique,
        data_type: "Utf8".into(),
    });
    for (row, v) in values.into_iter().enumerate() {
        if let Some(r) = table.rows.get_mut(row) {
            r.push(v);
        }
    }
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("anonymize failed: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(
        payload.to_string(),
    )]))
}
