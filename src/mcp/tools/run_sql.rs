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
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Map, Value};

use octa::db::pushdown::ServerSource;
use octa::formats::FormatRegistry;
use octa::sql::{
    AttachKind, QueryKind, SqlWorkspace, TableOrigin, WriteMode, WriteTarget, sanitize_sql_name,
};

use crate::mcp::OctaMcpServer;

use super::{Source, ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Run a DuckDB SQL query against a tabular source. The primary source (a file `path` or an \
`open_tab`) is registered as `data`. On a database tab that holds only part of its table the query runs on that server instead, in its SQL, and only reads. Use `extra_tables` to register more files for JOINs, \
`attach` to ATTACH whole DuckDB / SQLite files (`alias.schema.tbl`), and `write_to` to persist \
the SELECT result to a file - the extension picks the format: csv / tsv / parquet / json / xlsx \
(plain file, `table` is ignored) or duckdb / sqlite (a table inside the database, named by \
`table`). Give just a filename to write into the user's export folder. `limit` caps response \
rows (0 = unlimited). Pass explain: true to get the query plan instead of running the query.";

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

    /// Caps how many rows the *response* carries (the query still runs over
    /// every row). Omit to use the configured default; pass 0 for unlimited.
    /// Do NOT add a SQL `LIMIT` to `query` to control this - that truncates the
    /// computation itself. Set `unlimited` to also lift the file-loader cap so
    /// the query sees every row on disk.
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
    /// shape becomes `{ "kind": "write_back", "rows_written": N, "target": "..." }`,
    /// plus `initial_load_capped: true` and an `initial_load_note` when the row limit
    /// left rows behind.
    #[serde(default)]
    pub write_to: Option<WriteSpec>,

    /// Return the query's plan instead of running it: the steps the engine
    /// would take, the same in plain words and DuckDB's own plan text. Nothing runs, so
    /// this is safe on any query. Cannot be combined with `write_to`.
    #[serde(default)]
    pub explain: bool,
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

/// Whether `path` names a database file: a write there goes through DuckDB,
/// to anything else through the format registry.
fn is_database_path(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "duckdb" | "ddb" | "db" | "sqlite" | "sqlite3")
}

/// `query` with the tables it reads spelled out for the server: `data` is the tab's
/// table, and any other tab of this server in `extra_tables` goes by its name.
/// Common table expressions, so the statement is the caller's own, in the server's
/// SQL, and the server's planner pushes a filter down through them as it likes.
pub(super) fn server_statement(
    ctx: &ToolContext,
    p: &Params,
    src: &ServerSource,
) -> anyhow::Result<String> {
    if !p.attach.is_empty() {
        anyhow::bail!(
            "`attach` opens local files, and this query runs on the server of the database tab, \
             so one statement cannot do both."
        );
    }
    let mut tables = vec![("data".to_string(), src.from_sql())];
    for entry in &p.extra_tables {
        let other = ctx
            .db_tab(&Source::OpenTab(entry.path.to_string_lossy().into_owned()))
            .map(|(_, other)| other)
            .filter(|other| entry.table.is_none() && other.conn.id == src.conn.id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "`{}` is not a database tab on this server, so one statement cannot read both. \
                     query_db can join any tables that live on this server, by name.",
                    entry.path.display()
                )
            })?;
        tables.push((sanitize_sql_name(&entry.name), other.from_sql()));
    }
    let tables = tables
        .iter()
        .map(|(name, from)| format!("{name} AS (SELECT * FROM {from})"))
        .collect::<Vec<_>>()
        .join(", ");
    let query = p.query.trim().trim_end_matches(';').trim_end();
    Ok(match strip_word(query, "WITH") {
        Some(rest) => match strip_word(rest, "RECURSIVE") {
            Some(rest) => format!("WITH RECURSIVE {tables}, {rest}"),
            None => format!("WITH {tables}, {rest}"),
        },
        None => format!("WITH {tables} {query}"),
    })
}

/// `s` after its first word, when that is `word` in any case.
fn strip_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.get(word.len()..)?;
    (s[..word.len()].eq_ignore_ascii_case(word) && rest.starts_with(char::is_whitespace))
        .then(|| rest.trim_start())
}

/// A connection for the caller's SQL on a database tab's server, and the statement
/// once it is known not to write. This path only reads, whatever the profile or the
/// connection allow; `query_db` is the tool that changes data, under its own gates.
fn server_query(
    ctx: &ToolContext,
    p: &Params,
    src: &ServerSource,
) -> anyhow::Result<(Box<dyn octa::db::DbConnector>, String)> {
    let sql = server_statement(ctx, p, src)?;
    server_gate(src, &p.query, &sql)?;
    Ok((ctx.db_tab_connect(src)?, sql))
}

/// The write gate, as a read-only connection, on the caller's own statement and on
/// what the server is sent: the first word of a `DROP` is lost inside the WITH.
pub(super) fn server_gate(src: &ServerSource, query: &str, sql: &str) -> anyhow::Result<()> {
    let conn = octa::db::DbConnection {
        allow_writes: false,
        ..src.conn.clone()
    };
    octa::db::ensure_write_allowed(&conn, Some(query))?;
    octa::db::ensure_write_allowed(&conn, Some(sql))
}

fn run_on_server(ctx: &ToolContext, p: &Params, src: &ServerSource) -> anyhow::Result<Value> {
    let (mut c, sql) = server_query(ctx, p, src)?;
    if p.explain {
        return Ok(super::plan_json(&octa::sql::explain::explain_db(
            &mut *c, &sql,
        )?));
    }
    let row_cap = ctx.resolve_row_cap(p.limit);
    let result = super::query_for_response(&mut *c, &sql, row_cap, ctx.cell_byte_cap)?;
    let mut out = Map::new();
    out.insert("kind".to_string(), Value::String("select".to_string()));
    out.insert("on_server".to_string(), Value::Bool(true));
    out.insert("result".to_string(), result);
    Ok(Value::Object(out))
}

/// The whole result, for `write_to`: the one case that asks for every row, up to
/// the load cap.
fn result_for_write(
    ctx: &ToolContext,
    p: &Params,
    src: &ServerSource,
) -> anyhow::Result<octa::data::DataTable> {
    let (mut c, sql) = server_query(ctx, p, src)?;
    octa::db::with_identifier_fix(src.engine(), &sql, |s| c.query(s)).0
}

/// Says so when a `write_to` left rows behind: a partial file looks exactly like a
/// whole one once it is on disk, so the count alone would not tell anyone.
fn warn_if_short(
    out: &mut Map<String, Value>,
    rows: usize,
    cut: bool,
    server_hit_cap: bool,
    holds_part_of_source: bool,
) {
    let note = if cut {
        format!(
            "Only the first {rows} rows were written. The result has more, and the initial-load \
             row limit stopped it. Run this again with `unlimited: true` to write them all."
        )
    } else if server_hit_cap {
        format!(
            "Stopped at the initial-load row limit of {rows} rows, so the server's result may \
             hold more. Run this again with `unlimited: true` to write them all."
        )
    } else if holds_part_of_source {
        format!(
            "The {rows} rows written come from `data`, which holds only part of its source. \
             `unlimited: true` reads a whole file; an open tab has to load its whole table first."
        )
    } else {
        return;
    };
    // The load cap's own two fields. `truncated` is the response cap, a different
    // thing, and these rows are gone from the file, not only from the answer.
    out.insert("initial_load_capped".to_string(), Value::Bool(true));
    out.insert("initial_load_note".to_string(), Value::String(note));
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let _g = p
        .unlimited
        .then(|| octa::formats::InitialLoadRowsGuard::new(usize::MAX));

    // The plain answer. The seal below is what actually holds, whatever a
    // statement starts with.
    if ctx.read_only && !p.explain && octa::sql::is_mutation(&p.query) {
        anyhow::bail!(
            "Octa did not run this, because writes are off for this session (a read-only MCP \
             server or a chat profile without \"Allow writes\"). The statement changes data."
        );
    }

    let source = source_from(&p.open_tab, &p.path, &p.table);
    // A database tab holding part of its table: the server runs the query, which
    // is the point of SQL on one.
    let server = ctx.db_tab(&source).map(|(_, src)| src);
    if let Some(src) = server
        && p.write_to.is_none()
    {
        return run_on_server(ctx, p, src);
    }
    let mut ws = SqlWorkspace::new()?;
    // The one that matters most: registering a big file as a view over the
    // scan means an aggregate covers every row of it, where loading it would
    // answer from the first few million or run out of memory trying not to.
    let scanned = match &source {
        Source::Path { path, table: None }
            if ctx.scan_for(&source).is_some() && ws.add_view_from_scan("data", path).is_ok() =>
        {
            Some(path.as_path())
        }
        _ => None,
    };
    let streamed = scanned.is_some();
    let mut holds_part_of_source = false;
    let mut server_hit_cap = false;
    // With `write_to` the server's result becomes `data`, and the write below takes
    // all of it: `extra_tables` were part of the statement the server ran.
    let mut query = p.query.as_str();
    let (extra_tables, attach): (&[_], &[_]) = match server {
        Some(_) => (&[], &[]),
        None => (&p.extra_tables, &p.attach),
    };
    if let Some(src) = server {
        let result = result_for_write(ctx, p, src)?;
        // A connector stops at the cap and cannot look one row further, so a result
        // that reaches it may or may not be whole.
        server_hit_cap = result.row_count() >= octa::formats::initial_load_rows();
        ws.set_active_table(&result)?;
        query = "SELECT * FROM data";
    } else if !streamed {
        let active = ctx.resolve(&source)?;
        ws.set_active_table(&active)?;
        holds_part_of_source = active.is_partial();
    }

    for entry in extra_tables {
        let sql_name = sanitize_sql_name(&entry.name);
        // An extra table may itself be an open tab (addressed by handle / name /
        // file name) - register its in-memory snapshot so the model can JOIN any
        // number of open tabs. Otherwise read the file from disk (sandboxed).
        if entry.table.is_none()
            && let Some(snap) = ctx.snapshot_for_pathish(&entry.path.to_string_lossy())
        {
            ws.add_table(
                &sql_name,
                &ctx.resolve_snapshot(snap)?,
                TableOrigin::TabClone(snap.display_name.clone()),
            )?;
        } else {
            ctx.ensure_readable(&entry.path)?;
            ws.add_table_from_file(&entry.path, entry.table.as_deref(), &sql_name)?;
        }
    }
    for entry in attach {
        ctx.ensure_readable(&entry.path)?;
        let kind = AttachKind::from_path(&entry.path);
        ws.attach(&entry.path, &entry.alias, kind)?;
    }

    // `query` is SQL the caller wrote, and DuckDB SQL reads and writes files by
    // itself (`COPY ... TO`, `ATTACH`, `read_csv`). A read-only session must not
    // write and the assistant must not read beyond its open tabs, so what they
    // may not do, this workspace is made unable to do, before any of it runs. A
    // chat's `write_to` seals below, once the resolved target says how: a database
    // target needs one ATTACH of Octa's own first.
    if ctx.read_only || (ctx.restrict_filesystem && p.write_to.is_none()) {
        ws.seal(scanned)?;
    }

    if p.explain {
        if p.write_to.is_some() {
            anyhow::bail!("explain does not run the query, so it cannot write: drop write_to");
        }
        return Ok(super::plan_json(&ws.explain(&p.query)?));
    }
    if let Some(spec) = &p.write_to {
        // run_sql stays advertised read-only surfaces (it is a core read tool),
        // so its write branch must gate itself: --mcp-read-only and chat
        // profiles without writes both land here with `read_only` set.
        if ctx.read_only {
            anyhow::bail!("writes are disabled for this session; write_to is not available");
        }
        // A cloud URL resolves to a temp file; `dest.finish()` uploads it.
        let dest = ctx.resolve_write_dest(&spec.path)?;
        let target_path = dest.path();

        if is_database_path(target_path) {
            let mode = WriteMode::parse(&spec.mode)?;
            let target = WriteTarget {
                kind: AttachKind::from_path(target_path),
                path: target_path.to_path_buf(),
                schema: spec.schema.clone(),
                table: spec.table.clone(),
                mode,
                source_query: query.to_string(),
                create_schema_if_missing: spec.create_schema_if_missing,
            };
            let report = if ctx.restrict_filesystem {
                ws.write_sealed_result_to_db(&target, scanned)?
            } else {
                ws.write_result_to_db(&target)?
            };
            // Local: keep the rich "<file> (schema.table)" display. Cloud:
            // upload the temp DB and show the cloud URL.
            let target = if dest.is_cloud() {
                dest.finish()?
            } else {
                report.target_display.clone()
            };
            let mut out = Map::new();
            out.insert("kind".to_string(), Value::String("write_back".to_string()));
            out.insert("rows_written".to_string(), Value::from(report.rows_written));
            out.insert(
                "created_schema".to_string(),
                Value::Bool(report.created_schema),
            );
            out.insert("target".to_string(), Value::String(target));
            warn_if_short(
                &mut out,
                report.rows_written,
                report.cut,
                server_hit_cap,
                holds_part_of_source,
            );
            return Ok(Value::Object(out));
        }

        // File target (csv, tsv, parquet, json, xlsx, ...): run the SELECT and
        // write the result table via the format registry (overwrites the file).
        if ctx.restrict_filesystem {
            ws.seal(scanned)?;
        }
        let qo = ws.execute(query)?;
        let registry = FormatRegistry::new();
        let out_reader = registry.reader_for_path(target_path).ok_or_else(|| {
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
        if ctx.backup_before_modify && !dest.is_cloud() && target_path.exists() {
            octa::formats::backup_existing_file(target_path)?;
        }
        out_reader.write_file_schema_aware(
            target_path,
            &qo.table,
            ctx.allow_schema_changes,
            &Default::default(),
        )?;
        let rows_written = qo.table.row_count();
        let target = dest.finish()?;
        let mut out = Map::new();
        out.insert("kind".to_string(), Value::String("write_back".to_string()));
        out.insert("rows_written".to_string(), Value::from(rows_written));
        out.insert("target".to_string(), Value::String(target));
        warn_if_short(
            &mut out,
            rows_written,
            qo.cut,
            server_hit_cap,
            holds_part_of_source,
        );
        return Ok(Value::Object(out));
    }

    let qo = ws.execute(query)?;
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
    // A CREATE TABLE / VIEW returns the created table's rows as `result`.
    if let Some(name) = qo.created.as_ref() {
        out.insert("created".to_string(), Value::String(name.clone()));
    }
    if streamed {
        // `data` was a view over the file, so the answer covers every row of
        // it rather than the first few million.
        out.insert("streamed".to_string(), Value::Bool(true));
    }
    if holds_part_of_source {
        // The result is complete, so `table_to_json` cannot tell: `data` was not.
        out.insert("initial_load_capped".to_string(), Value::Bool(true));
        out.insert(
            "initial_load_note".to_string(),
            Value::String(
                "`data` holds only part of its source, so this answer covers that part, not the \
                 whole source."
                    .to_string(),
            ),
        );
    }
    out.insert("result".to_string(), table_value);
    Ok(Value::Object(out))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || super::say_if_partial(|| run(&ctx, &p)))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e:#}"), None))?
        .map_err(|e| McpError::invalid_params(format!("run_sql failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}

#[cfg(test)]
#[path = "run_sql_tests.rs"]
mod tests;
