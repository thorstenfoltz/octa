//! MCP (Model Context Protocol) server for Octa, exposed via `octa --mcp`.
//!
//! The server is a stdio JSON-RPC endpoint built on `rmcp`. It re-uses the
//! library crate's `FormatRegistry` to read any of the formats Octa supports
//! in the GUI, plus `octa::sql::run_query` for DuckDB execution.
//!
//! ## Modular tool layout
//!
//! Every tool lives in its own file under `src/mcp/tools/`. The
//! `OctaMcpServer` impl in this file is a thin dispatcher - each `#[tool]`
//! method delegates to `tools::<name>::handle`. Adding a new tool is a
//! drop-in: create `tools/foo.rs` with `Params` + `handle`, register the
//! module in `tools/mod.rs`, and add a wrapper method below.
//!
//! Tool descriptions are inlined as string literals at the `#[tool]` site
//! (rmcp's macro doesn't accept a `const &str` there) - keep them in sync
//! with the per-tool docstrings.
//!
//! ## Row + cell limits
//!
//! The MCP server runs blocking work on `tokio::task::spawn_blocking` so it
//! doesn't park the rmcp runtime. Every result-bearing tool honours the
//! server's configured row cap (default 1000, override via `AppSettings.
//! mcp_default_row_limit`) and cell-size cap (default 64 KiB,
//! `AppSettings.mcp_default_cell_bytes`). Both can be overridden per-call
//! via the tool's `limit` parameter and respond with `truncated` /
//! `cell_truncated` flags so the model can re-query for more.

pub mod tools;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::transport::stdio;
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt, tool, tool_handler, tool_router};

// The numeric defaults (1000 rows, 64 KiB per cell) live in
// `src/ui/settings.rs::default_mcp_row_limit` / `default_mcp_cell_bytes`.
// `OctaMcpServer::new` receives the resolved values from AppSettings, so
// there's no second copy of them to drift.

/// Octa's MCP server state. Holds the configured row + cell caps plus the
/// rmcp tool router. Cloneable so rmcp can fan out per-request handlers.
#[derive(Clone)]
pub struct OctaMcpServer {
    /// Default row cap applied when the caller omits `limit`. `None` means
    /// no cap (return every row). Set by AppSettings at server startup.
    pub default_row_limit: Option<usize>,
    /// Per-cell byte cap. `0` means no cap.
    pub cell_byte_cap: usize,
    /// Local files at least this big are scanned in place by the tools that
    /// can answer without the rows. Read once from `AppSettings` at startup.
    pub large_file_min_bytes: usize,
    /// Permit schema-changing DuckDB/SQLite/GeoPackage saves. Read once from
    /// `AppSettings` (`!write_protection`) at server startup.
    pub allow_schema_changes: bool,
    /// Back up an existing file before modifying it in place.
    pub backup_before_modify: bool,
    /// `--mcp-read-only`: write tools are dropped from the router and
    /// `query_db` refuses mutations.
    pub read_only: bool,
    /// Saved live-database connections, read once at startup.
    pub db_connections: Vec<octa::db::DbConnection>,
    /// rmcp tool routing table (populated by `#[tool_router]`).
    pub tool_router: ToolRouter<OctaMcpServer>,
}

impl OctaMcpServer {
    /// Build a [`tools::ToolContext`] for one tool call. The MCP server has no
    /// open GUI tabs, so the context carries only the configured caps; the
    /// in-GUI chat agent builds a context with tab snapshots instead. Sharing
    /// the type lets both surfaces call the same `tools::<name>::run`.
    pub fn tool_context(&self) -> tools::ToolContext {
        tools::ToolContext::for_mcp(
            self.default_row_limit,
            self.cell_byte_cap,
            self.allow_schema_changes,
            self.backup_before_modify,
            self.db_connections.clone(),
            self.read_only,
            self.large_file_min_bytes,
        )
    }
}

#[tool_router]
impl OctaMcpServer {
    pub fn new(
        default_row_limit: Option<usize>,
        cell_byte_cap: usize,
        read_only: bool,
        allow_schema_changes: bool,
        backup_before_modify: bool,
        large_file_min_bytes: usize,
    ) -> Self {
        let mut tool_router = Self::tool_router();
        if read_only {
            // Read-only mode: drop every tool that mutates a file so the
            // server can be wired into agent frameworks with no write surface.
            for name in [
                "write_table",
                "edit_table",
                "convert",
                "transform_columns",
                "anonymize",
                "partition_table",
                "write_db_table",
                "copy_db_table",
                "write_workbook",
                "copy_object",
                "batch_convert",
                "move_object",
                "delete_object",
                "create_report",
                "harmonise_schemas",
            ] {
                tool_router.remove_route(name);
            }
        }
        Self {
            default_row_limit,
            cell_byte_cap,
            large_file_min_bytes,
            allow_schema_changes,
            backup_before_modify,
            read_only,
            // Settings are read once at server startup, like the caps above.
            db_connections: octa::ui::settings::AppSettings::load().db_connections,
            tool_router,
        }
    }

    // NOTE: rmcp's `#[tool(description = ...)]` macro only accepts a string
    // literal, so the descriptions are inlined here rather than pulled from
    // the per-tool modules' `DESCRIPTION` consts. The consts stay around for
    // tests / future reuse and should be kept in sync with what's below.
    #[tool(
        description = "Read a tabular data file and return the column schema and rows. Supports \
Parquet, CSV, TSV, JSON, JSONL, Excel, SQLite, DuckDB, GeoPackage, ORC, Avro, Arrow IPC, SAS, \
SPSS, Stata, RDS, HDF5, NetCDF, DBF, plus text formats (XML, TOML, YAML, Markdown, Jupyter). \
Parquet files with very many row groups fall back to a DuckDB-backed reader. \
Returns JSON with `schema`, `rows`, `row_count`, `truncated`, `total_rows_available`, \
`cell_truncated`. Pass `limit: 0` for unlimited response rows; pass `unlimited: true` to \
also lift the 5,000,000-row file-loader cap so every row is read from disk. Use both together \
to truly return every row."
    )]
    async fn read_table(
        &self,
        Parameters(p): Parameters<tools::read_table::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::read_table::handle(self, p).await
    }

    #[tool(
        description = "Read a tabular data file and return its LAST N rows (the tail), same \
response shape as `read_table`: `{schema, rows, row_count, ...}`. `limit` sets N (default the \
server's configured row limit; 0 = the whole loaded window). For multi-table sources pass \
`table`. Streaming readers load with the 5,000,000-row cap, so the tail reflects the end of \
that window; pass `unlimited: true` to reach the true end of a very large file."
    )]
    async fn tail(
        &self,
        Parameters(p): Parameters<tools::tail::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::tail::handle(self, p).await
    }

    #[tool(
        description = "Read a tabular data file and return a random N-row sample (without \
replacement, original row order preserved), same response shape as `read_table`. `limit` sets \
the sample size (default the server's configured row limit; 0 = every row, no sampling). \
`seed` makes the sample reproducible (default 0). For multi-table sources pass `table`; pass \
`unlimited: true` so the sample is drawn from every row on disk rather than just the loaded \
window."
    )]
    async fn sample(
        &self,
        Parameters(p): Parameters<tools::sample::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::sample::handle(self, p).await
    }

    #[tool(
        description = "Return the column schema (name + data type) of a tabular file. The response \
contains only schema metadata - no rows are serialised - though the file is still loaded through \
the standard reader (subject to the initial-load cap for streaming formats). Cheap to call as a \
discovery step before `read_table` or `run_sql`. For multi-table sources, pass the `table` \
parameter to get a specific table's schema."
    )]
    async fn schema(
        &self,
        Parameters(p): Parameters<tools::schema::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::schema::handle(self, p).await
    }

    #[tool(
        description = "List the tables inside a multi-table container (SQLite, DuckDB, \
GeoPackage). Returns `tables` as an array of `{name, columns, row_count}` objects. For \
single-table file formats this returns an empty list - call `schema` or `read_table` directly \
instead."
    )]
    async fn list_tables(
        &self,
        Parameters(p): Parameters<tools::list_tables::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::list_tables::handle(self, p).await
    }

    #[tool(
        description = "List one folder level of a cloud object store by URL: `s3://bucket/prefix`, \
`az://container/prefix`, or `gs://bucket/prefix` (empty prefix = bucket root). Returns `objects` \
as `{name, key, url, is_folder, size, modified}`; open a listed file with `read_table` (or any \
read tool) by passing its `url` as `path`. Read tools accept cloud URLs directly. Credentials use \
the ambient chain: AWS_* env vars, a cached SSO session, Azure CLI login, or Google \
application-default credentials. Azure needs AZURE_STORAGE_ACCOUNT set (an az:// URL cannot carry \
the account name)."
    )]
    async fn list_objects(
        &self,
        Parameters(p): Parameters<tools::list_objects::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::list_objects::handle(self, p).await
    }

    #[tool(
        description = "Copy a cloud object, or every object under a prefix, to another location. \
`from` and `to` are cloud URLs (`s3://bucket/key`, `az://container/key`, `gs://bucket/key`). A \
`from` ending in `/` copies the whole folder recursively and recreates its shape under `to`. \
Source and destination may be different buckets, accounts or providers (S3 -> GCS works); a copy \
inside one bucket is server-side, a copy across buckets is streamed in blocks so object size does \
not drive memory. The source is never modified. Refuses to copy a folder into itself, and refuses \
more than 10,000 objects in one call."
    )]
    async fn copy_object(
        &self,
        Parameters(p): Parameters<tools::copy_object::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::copy_object::handle(self, p).await
    }

    #[tool(
        description = "Move a cloud object, or every object under a prefix, to another location: \
a copy followed by deleting the source. `from` and `to` are cloud URLs; a `from` ending in `/` \
moves the whole folder recursively. Works across buckets, accounts and providers. Object stores \
have no rename, so this is genuinely copy-then-delete: the delete only runs after every copy \
succeeded. Refuses more than 10,000 objects in one call. Use `copy_object` when the source should \
survive."
    )]
    async fn move_object(
        &self,
        Parameters(p): Parameters<tools::move_object::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::move_object::handle(self, p).await
    }

    #[tool(
        description = "Delete a cloud object, or every object under a prefix. `url` is a cloud \
URL; ending it in `/` deletes the whole folder recursively, which additionally requires \
`recursive: true`. **This cannot be undone** unless the bucket has versioning on. Deleting a key \
that does not exist is not an error. Refuses more than 10,000 objects in one call. Prefer \
`move_object` to an archive prefix when the data might still be wanted."
    )]
    async fn delete_object(
        &self,
        Parameters(p): Parameters<tools::delete_object::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::delete_object::handle(self, p).await
    }

    #[tool(
        description = "List the saved live-database connections (Settings -> Databases): name, \
engine (PostgreSQL / MySQL / SQL Server), host, port, database, and whether writes are allowed. \
Use a connection's `name` with `list_db_tables`, `query_db`, `write_db_table`, and \
`copy_db_table`. Read-only; does not contact any server."
    )]
    async fn list_db_connections(
        &self,
        Parameters(p): Parameters<tools::list_db_connections::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::list_db_connections::handle(self, p).await
    }

    #[tool(
        description = "List the schemas and tables of a saved live-database connection (see \
`list_db_connections` for the names). Pass `schema` to list only that schema's tables. Returns \
`tables` as an array of `{schema, table}`. Query a listed table with `query_db` (e.g. SELECT * \
FROM schema.table). Read-only."
    )]
    async fn list_db_tables(
        &self,
        Parameters(p): Parameters<tools::list_db_tables::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::list_db_tables::handle(self, p).await
    }

    #[tool(
        description = "Read the foreign keys a live database declares, so you learn how its \
tables connect without reading a single row. Takes a saved `connection` (see \
`list_db_connections`), optional `catalog` for Snowflake/Databricks/BigQuery, and `schemas` \
(default: every schema). Returns `relationships`, each naming the child and parent table and \
column plus the `constraint` name. Postgres, MySQL, SQL Server and Exasol enforce their foreign \
keys, so an edge from those is also true of the rows; Redshift, Snowflake, Databricks and \
BigQuery accept a declaration and enforce nothing. Pass `measure: true` to read a sample of rows \
and add `overlap`, `score` and orphan counts both ways round per edge, which is how you find a \
declared key nothing honours (a declared key is measured child to parent, so `left_orphans` is \
the child rows pointing at a parent that does not exist). ClickHouse has no foreign keys at all. Read-only."
    )]
    async fn db_relationships(
        &self,
        Parameters(p): Parameters<tools::db_relationships::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::db_relationships::handle(self, p).await
    }

    #[tool(
        description = "Run one SQL statement on a saved live-database connection (see \
`list_db_connections`), server-side and in the engine's NATIVE dialect (PostgreSQL, \
MySQL/MariaDB, or SQL Server - not DuckDB SQL). SELECTs return `{schema, rows, ...}` like \
`read_table` (the `limit` param caps the response). Mutations (INSERT/UPDATE/DELETE/DDL) return \
`{rows_affected}` and are refused unless the connection's \"Allow writes\" switch is on. Add \
your own LIMIT/TOP for large tables - the whole result is fetched from the server."
    )]
    async fn query_db(
        &self,
        Parameters(p): Parameters<tools::query_db::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::query_db::handle(self, p).await
    }

    #[tool(
        description = "Return the SQL that would make a live database table match a file or an \
open tab, WITHOUT running it. Use when a change to a server table has to be reviewed before it \
is applied. Reads the table (see `list_db_connections`), compares it to the source on the `on` \
key columns, and returns one transaction: DELETEs for rows only on the server, full-row UPDATEs \
for rows whose values differ, INSERTs for rows only in the source. Nothing is written. Columns \
present only in the source are reported in `ignored_columns` and skipped - this never emits \
ALTER TABLE. Numbers are compared as numbers, so 120.50 and 120.5 are not a change."
    )]
    async fn sync_sql(
        &self,
        Parameters(p): Parameters<tools::sync_sql::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::sync_sql::handle(self, p).await
    }

    #[tool(
        description = "Write several tables into ONE .xlsx workbook, one worksheet per entry. \
Each sheet names a source: `path` (any readable file) or `open_tab` (in-GUI assistant only), \
plus an optional sheet `name` (default: the file stem or tab name). Sheet names are corrected \
to Excel's rules automatically - at most 31 characters, no forbidden punctuation, duplicates \
numbered - so a write cannot produce a workbook Excel refuses to open. Use this instead of \
calling `convert` once per table when the user wants one file."
    )]
    async fn write_workbook(
        &self,
        Parameters(p): Parameters<tools::write_workbook::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::write_workbook::handle(self, p).await
    }

    #[tool(
        description = "Write a table into a saved live-database connection (see \
`list_db_connections`): source rows come from `path` (any readable file, cloud URLs included) \
or `open_tab` (in-GUI assistant only). Target is `schema` + `table`; `mode` is `create` \
(default, error if the table exists), `append`, or `replace` (DROP + CREATE). Refused unless \
the connection's \"Allow writes\" switch is on. Returns `{rows_written, created}`."
    )]
    async fn write_db_table(
        &self,
        Parameters(p): Parameters<tools::write_db_table::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::write_db_table::handle(self, p).await
    }

    #[tool(
        description = "Copy a table from one saved live-database connection to another (see \
`list_db_connections`), streamed server-to-server through DuckDB - fast for large tables and \
not limited by any row cap. Postgres and MySQL/MariaDB only (SQL Server is not supported); the \
target connection's \"Allow writes\" switch must be on. `mode` is `create` (default, error if \
the target table exists), `append`, or `replace` (DROP + CREATE). Target schema/table default \
to the source's. Returns `{rows_copied, created}`."
    )]
    async fn copy_db_table(
        &self,
        Parameters(p): Parameters<tools::copy_db_table::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::copy_db_table::handle(self, p).await
    }

    #[tool(
        description = "Count rows in a tabular file. Loads the table and reports its row count. \
For streaming formats (Parquet, CSV, TSV) the count is bounded by Octa's 5,000,000-row \
initial-load cap; the response flags `initial_load_capped: true` when the count may not \
reflect every row in the source. Pass `unlimited: true` to lift the cap and get the true \
total. A large Parquet, CSV or JSON file is counted exactly straight from the file without \
reading it, and the response then carries `streamed: true`."
    )]
    async fn count_rows(
        &self,
        Parameters(p): Parameters<tools::count_rows::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::count_rows::handle(self, p).await
    }

    #[tool(
        description = "Run a DuckDB SQL query against one or more files using the multi-table \
SQL workspace. The primary `path` file is loaded and registered as `data`. Use `extra_tables` \
to register additional files (any format Octa supports) under SQL identifiers so the query can \
JOIN across heterogeneous sources. Use `attach` to ATTACH whole DuckDB or SQLite files so \
their tables are queryable as `alias.schema.tbl` without row copies. Use `write_to` to write \
the SELECT result back into a DuckDB or SQLite file (target schema + table + mode \
`create|replace|append`); the response then becomes `{ kind: 'write_back', rows_written, \
created_schema, target }`. For row-returning queries the response is `{ kind: 'select' | \
'mutation', result, affected? }` carrying the same `truncated` / `cell_truncated` flags as \
`read_table`. Pass `limit: 0` for unlimited response rows; pass `unlimited: true` to also \
lift the 5,000,000-row file-loader cap so every loaded file is read in full."
    )]
    async fn run_sql(
        &self,
        Parameters(p): Parameters<tools::run_sql::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::run_sql::handle(self, p).await
    }

    #[tool(
        description = "Convert a file from one tabular format to another. Both ends are \
resolved by file extension. The output extension must map to a writable format - read-only \
formats (SAS, RDS, HDF5, NetCDF) cannot be a target. The input is read with the streaming \
initial-load cap (5,000,000 rows by default); pass `unlimited: true` to convert the entire \
source. Returns the row/column count and the output path on success."
    )]
    async fn convert(
        &self,
        Parameters(p): Parameters<tools::convert::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::convert::handle(self, p).await
    }

    #[tool(
        description = "Generate a schema artifact from a tabular file: SQL DDL for Postgres, \
MySQL, SQLite, Databricks, or Snowflake, or a Pydantic v2 model, a TypeScript interface, a \
JSON Schema document, or a Rust struct. Pick the output with the `target` parameter \
(`postgres`, `mysql`, `sqlite`, `databricks`, `snowflake`, `pydantic`, `typescript`, \
`json-schema`, `rust`). Returns `target`, `table_name`, `column_count`, and the generated \
`code`. Only the column schema is read - no rows are serialised."
    )]
    async fn export_schema(
        &self,
        Parameters(p): Parameters<tools::export_schema::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::export_schema::handle(self, p).await
    }

    #[tool(
        description = "Profile a tabular file: per-column statistics via DuckDB's SUMMARIZE \
- data type, min, max, approximate distinct count, mean, standard deviation, q25/q50/q75, \
row count, and null percentage. Returns `columns` as an array of per-column stat objects. \
The fastest way to understand an unfamiliar dataset before reading rows or writing SQL. \
Stats reflect at most the first 5,000,000 rows by default; pass `unlimited: true` to \
profile the full file."
    )]
    async fn profile(
        &self,
        Parameters(p): Parameters<tools::profile::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::profile::handle(self, p).await
    }

    #[tool(
        description = "Find duplicate rows in a tabular file. `key_columns` lists the column \
names whose combined value forms the duplicate key; every row sharing its key with at least \
one other row is returned. The response carries `duplicate_row_count` and `result` (schema \
+ the duplicate rows, honouring the row/cell caps). Pass `limit: 0` for unlimited response \
rows; pass `unlimited: true` to also lift the 5,000,000-row file-loader cap so duplicate \
detection considers every row in the file."
    )]
    async fn find_duplicates(
        &self,
        Parameters(p): Parameters<tools::find_duplicates::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::find_duplicates::handle(self, p).await
    }

    #[tool(
        description = "Find near-duplicate rows (fuzzy): rows almost the same on the chosen \
columns (typos, spacing, reordered words). `key_columns` are compared (averaged); `method` is \
edit_ratio / jaro_winkler / token_set; `threshold` 0.0..=1.0 (default 0.85). Optional \
`block_column` only compares rows sharing its exact value. Returns clusters (`{rows, score}`), \
`compared_rows`, and `capped`. Read-only analytics."
    )]
    async fn fuzzy_duplicates(
        &self,
        Parameters(p): Parameters<tools::fuzzy_duplicates::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::fuzzy_duplicates::handle(self, p).await
    }

    #[tool(
        description = "Count how often each value appears in one column of a tabular file - \
a `value_counts()` equivalent. Returns `rows` (label + count, most frequent first) plus \
`nulls`, `total_non_null`, and `unique_count`. Set `bin: true` to group a numeric column \
into Sturges bins instead of counting raw values; use `top_n` to cap the returned rows. \
Counts reflect at most the first 5,000,000 rows by default; pass `unlimited: true` to \
scan the full file."
    )]
    async fn value_frequency(
        &self,
        Parameters(p): Parameters<tools::value_frequency::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::value_frequency::handle(self, p).await
    }

    #[tool(
        description = "Search every cell of a tabular file for a query string. `mode` \
selects `plain` (case-insensitive substring, default), `wildcard` (`*` / `?`), or `regex`. \
Returns `hits` as `{row, col, column_name, snippet}` objects plus `hit_count` and \
`truncated`. Pass `limit: 0` for unlimited hits; pass `unlimited: true` to also lift the \
5,000,000-row file-loader cap so the search scans every row in the file."
    )]
    async fn search(
        &self,
        Parameters(p): Parameters<tools::search::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::search::handle(self, p).await
    }

    #[tool(
        description = "Compare the column schemas of two tabular files. Reads each file's \
column metadata only (no row data) and returns the four-way diff: `common` (columns with \
matching name and type), `only_in_a`, `only_in_b`, and `type_mismatches` (same column name, \
different `data_type`). Pair this with `export_schema` / `validate_against_schema` for \
schema-drift workflows across file versions. For multi-table sources, pass `table_a` and / \
or `table_b` to choose specific tables. Returns `{ identical, common, only_in_a, only_in_b, \
type_mismatches }`."
    )]
    async fn compare_schemas(
        &self,
        Parameters(p): Parameters<tools::compare_schemas::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::compare_schemas::handle(self, p).await
    }

    #[tool(
        description = "Row-level diff of two tabular files. Reads both files and compares rows \
by whole-row content (every column, positionally), so the two files should share the same \
column order for a meaningful result. Returns `only_in_a` and `only_in_b` (each a table \
payload of the rows unique to that side: `{schema, rows, row_count, truncated, ...}`), plus \
`only_in_a_count`, `only_in_b_count`, and `shared_keys` (distinct row keys present in both). \
For multi-table sources pass `table_a` / `table_b`. `limit` caps rows returned per side (0 = \
unlimited); `unlimited: true` also lifts the 5,000,000-row file-loader cap. Use \
`compare_schemas` first if the column layouts might differ."
    )]
    async fn diff_tables(
        &self,
        Parameters(p): Parameters<tools::diff_tables::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::diff_tables::handle(self, p).await
    }

    #[tool(
        description = "Validate a tabular file's column schema against an expected JSON \
Schema (typically one produced by `export_schema --target json-schema`). Returns `matches` \
(true when every column lines up by name and type), `diff` (a full SchemaDiff with `common`, \
`only_in_a`, `only_in_b`, `type_mismatches`), and `unparsed_types` (JSON Schema type values \
the parser could not map to an Arrow type - those columns default to `Utf8`). Provide the \
expected schema via `schema_path` (a file path) OR `schema_inline` (the JSON text); exactly \
one of the two is required. Use this to gate data ingestion in a CI / pipeline step after \
locking in a schema with `export_schema`."
    )]
    async fn validate_against_schema(
        &self,
        Parameters(p): Parameters<tools::validate_schema::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::validate_schema::handle(self, p).await
    }

    #[tool(
        description = "One-shot orientation snapshot of a tabular file. Collapses the usual \
`list_tables` -> `schema` -> `read_table` discovery dance into a single call. Returns `path`, \
`format_name`, `file_size_bytes`, `table`, `row_count`, `initial_load_capped`, \
`initial_load_cap`, `columns` (schema), `column_count`, `sample_rows` (first N rows), \
`sample_row_count`, `cell_truncated`. Use this as the first call when meeting an unfamiliar \
file. `sample_rows` defaults to 5 (max 100). For multi-table sources pass `table`; without \
it the reader's default table behaviour applies. Pass `unlimited: true` to lift the \
5,000,000-row file-loader cap if you need an accurate row count for a very large file."
    )]
    async fn describe_file(
        &self,
        Parameters(p): Parameters<tools::describe_file::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::describe_file::handle(self, p).await
    }

    #[tool(
        description = "Find columns (and optional small combinations) whose values are \
unique across a tabular file. Useful for primary-key reconnaissance on undocumented sources. \
Returns `total_rows`, `single` (per-column results with `column`, `distinct_count`, \
`null_count`, `is_unique`), and `combos` (multi-column results when `max_combo_size > 1`). \
`is_unique` is true only when every row contributes a distinct value AND there are no \
nulls - most databases reject NULL in a primary key. `max_combo_size` is clamped to `[1, 3]` \
(default 1); combo tests skip columns that are already unique on their own or carry only \
one distinct value. Pass `unlimited: true` to scan the full file."
    )]
    async fn unique_columns(
        &self,
        Parameters(p): Parameters<tools::unique_columns::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::unique_columns::handle(self, p).await
    }

    #[tool(
        description = "Check a table's values against a TOML rules file and report which rules \
failed. Rules name their columns, and each supports one check: `not_null`, `unique`, `range` \
(min/max), `regex` (pattern) or `max_length`. The response carries `passed`, one entry per failing \
rule with its failure count and up to three offending values, and `unknown` for rules naming \
columns the table does not have. A rule that cannot run counts as a failure, never as a pass. Use \
it to verify an extract before trusting it; the same file works from the command line as `octa \
--check FILE --rules RULES`."
    )]
    async fn check_rules(
        &self,
        Parameters(p): Parameters<tools::check_rules::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::check_rules::handle(self, p).await
    }

    #[tool(
        description = "Compare two versions of the same dataset and report how it moved: columns \
added or removed, and per shared column the change in null rate, distinct count and (for numeric \
columns) minimum, maximum and mean. Columns with few distinct values also report which category \
values appeared and vanished. Use it to answer whether today's extract still looks like \
yesterday's, before trusting a load. Pass `fail_on` (e.g. `null_rate:0.05,rows:0.1`) to have the \
response's `failed` flag act as a gate. Either path may be an open tab or a cloud object URL. \
This measures distributions, not rows: use `diff_tables` when you need to know which rows changed."
    )]
    async fn data_drift(
        &self,
        Parameters(p): Parameters<tools::data_drift::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::data_drift::handle(self, p).await
    }

    #[tool(
        description = "Scan a folder of data files and report which of them disagree about their \
columns. Files are grouped by identical schema, so 500 Parquet parts come back as a handful of \
variants rather than 500 entries. Returns `has_drift`, `variants` (each with its file list and \
columns), `drifting_columns` and `skipped`. Use it when a union or dataset open failed with a \
type error that named no file."
    )]
    async fn schema_drift(
        &self,
        Parameters(p): Parameters<tools::schema_drift::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::schema_drift::handle(self, p).await
    }

    #[tool(
        description = "Write a self-contained HTML profiling report for a table: per-column \
statistics, distribution charts, the most common values and a correlation matrix. The document \
embeds its own CSS and SVG and fetches nothing, so it can be mailed or published as-is. \
`out_path` is where to write it. `sections` picks a subset from stats, distributions, \
top_values and correlation (default: all four). `sample_rows` profiles a random sample instead \
of every row, and the report states that it did."
    )]
    async fn create_report(
        &self,
        Parameters(p): Parameters<tools::create_report::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::create_report::handle(self, p).await
    }

    #[tool(
        description = "Join sources on how similar their values are rather than on exact \
equality, for tables that name the same thing differently (\"Mueller GmbH\" against \"Mueller \
Gmbh.\"). Each entry in `sources` has a `path` or `open_tab`; they are folded left to right. \
`on` gives the column pairs to compare as \"LEFT=RIGHT\" (repeat to average several columns). \
`method` is edit_ratio (default), jaro_winkler or token_set; `threshold` is the minimum average \
score in 0..1 (default 0.85); `block` (\"LEFT=RIGHT\") restricts comparison to rows that agree \
exactly on those columns, which is what makes a large join feasible. Each left row keeps its \
single best partner. The result adds match_score_N and ambiguous_N per step: ambiguous means \
the runner-up scored nearly as well, so that match is the one to check. Use suggest_join_keys \
first if you do not know which columns to compare."
    )]
    async fn fuzzy_join(
        &self,
        Parameters(p): Parameters<tools::fuzzy_join::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::fuzzy_join::handle(self, p).await
    }

    #[tool(
        description = "Rank the column pairs that would actually join two or more tables, by how \
much their values overlap, weighted by distinctness so a status column cannot outrank a real \
key. Use before `join_tables` when the key columns have different names or are unknown. Takes \
`paths` and/or `open_tabs` (two or more sources in total). Returns `candidates` best first, \
each naming both sides plus `overlap`, `left_distinct`, `right_distinct`, `score`, and orphan \
counts BOTH ways round: `left_orphans` out of `left_distinct_values` and `right_orphans` out of \
`right_distinct_values`, how many distinct values on each side find no partner on the other. \
Orphans are what separate two candidates that score identically, which happens whenever both \
tables number their rows from 1; only the count read from the CHILD side separates them, so \
check both. Sampled (`sample`, \
default 10000 rows per table), so a high overlap is strong evidence rather than proof. \
Read-only."
    )]
    async fn suggest_join_keys(
        &self,
        Parameters(p): Parameters<tools::suggest_join_keys::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::suggest_join_keys::handle(self, p).await
    }

    #[tool(
        description = "Explain why a join between two key columns returns fewer rows than \
expected. Returns matched and unmatched key counts per side, sample unmatched values, and \
which single normalisation would increase the match: trim_whitespace, ignore_case, \
collapse_whitespace, strip_punctuation or strip_leading_zeros. A fix is reported only when it \
strictly beats the current match count, so an empty `fixes` list means the columns genuinely \
hold different things. Counts are over distinct keys, not rows, and are sampled (`sample`, \
default 10000 rows per side). Use after `suggest_join_keys` and before `join_tables`. \
Read-only: diagnoses only, changes neither table."
    )]
    async fn diagnose_join(
        &self,
        Parameters(p): Parameters<tools::diagnose_join::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::diagnose_join::handle(self, p).await
    }

    #[tool(
        description = "Rewrite every data file in a folder to one common set of columns, \
writing harmonised copies into a separate folder. The originals are NEVER modified. The target \
schema is the shape most files already have, or the schema of `target_file` when given. Columns \
missing from a file are added as nulls; columns not in the target are dropped and listed per \
file. A file whose values cannot be cast to a target type is REFUSED rather than written with \
nulls in place of those values, so a harmonised folder never contains silently emptied cells. \
Returns a per-file report plus `written` and `refused` counts. Write tool: creates files."
    )]
    async fn harmonise_schemas(
        &self,
        Parameters(p): Parameters<tools::harmonise_schemas::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::harmonise_schemas::handle(self, p).await
    }

    #[tool(
        description = "Write model-supplied rows to a file in any writable format - the inverse \
of `read_table`. Pick the format with the output extension (`.csv`, `.parquet`, `.json`, \
`.xlsx`, ...); read-only formats (SAS, RDS, HDF5, NetCDF) cannot be a target. Supply `columns` \
(name + optional Arrow `type`, defaulting to `Utf8`) and `rows` as an array-of-arrays lined up \
positionally with the columns - the same shape `read_table` returns, so a read result \
round-trips straight back in. `mode` is `create` (default; errors if the file exists), \
`overwrite` (replace the whole file), or `append` (the file must exist and its column names \
must match; the new rows are added to the end). Database files (`.sqlite` / `.duckdb`) are not \
valid targets here - use `edit_table` or `run_sql` with `write_to`. Returns `rows_written`, \
`cols_written`, `output`, and `mode`."
    )]
    async fn write_table(
        &self,
        Parameters(p): Parameters<tools::write_table::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::write_table::handle(self, p).await
    }

    #[tool(
        description = "Edit an existing tabular file in place and save it back through its native \
writer. `set` updates individual cells (`row` is 0-based; `col` is a 0-based index or a column \
name); `insert_rows` adds rows (`at` is the 0-based insertion index, omit to append; `values` \
line up with the columns); `delete_rows` removes rows by 0-based index. SQLite / DuckDB sources \
keep diff-based save semantics - only changed rows are UPDATE/INSERT/DELETE-d - so editing a \
few cells does not rewrite the whole table. Column changes (rename / add / drop) are not \
supported. Use `table` to pick the table on multi-table sources, and `unlimited: true` to load \
the entire file before editing. Returns `cells_set`, `rows_inserted`, `rows_deleted`, and \
`path`."
    )]
    async fn edit_table(
        &self,
        Parameters(p): Parameters<tools::edit_table::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::edit_table::handle(self, p).await
    }

    #[tool(
        description = "Reshape a table between long and wide form using DuckDB PIVOT / UNPIVOT. \
With `mode: \"pivot\"` (default), spread the distinct values of column `on` into new columns, \
aggregating `value` with `agg` (sum/count/avg/min/max), optionally grouped by `group`. With \
`mode: \"unpivot\"`, melt the columns in `columns` into two columns named `name_col` / \
`value_col`. Returns the reshaped table (`{schema, rows, row_count, ...}`). Operates on a file \
`path` or an `open_tab`."
    )]
    async fn pivot(
        &self,
        Parameters(p): Parameters<tools::pivot::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::pivot::handle(self, p).await
    }

    #[tool(
        description = "Group rows into time buckets: one output row per interval of a timestamp \
column. `time_col` is the timestamp column, `value_cols` the columns aggregated, `interval` one \
of minute/hour/day/week/month/quarter/year (default day), `agg` one of \
sum/mean/min/max/count/first/last (default sum). `group_by` gives one series per combination. \
Returns the resampled table. Operates on a file `path` or an `open_tab`."
    )]
    async fn resample_timeseries(
        &self,
        Parameters(p): Parameters<tools::resample::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::resample::handle(self, p).await
    }

    #[tool(
        description = "Convert several files into one target format in a single run. `inputs` \
is the list of source paths, `out_dir` the output directory, `to` the target extension without \
a dot. `overwrite` replaces existing outputs (default: skip). Gzip and zstd inputs decompress \
automatically. One failure does not stop the run; the response reports \
`{converted, failed, skipped, items}`."
    )]
    async fn batch_convert(
        &self,
        Parameters(p): Parameters<tools::batch_convert::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::batch_convert::handle(self, p).await
    }

    #[tool(
        description = "Add a rolling aggregate column over the previous N rows. `order_col` \
orders the frame (required), `value_col` is aggregated, `window` is the frame size including \
the current row, `agg` one of sum/mean/min/max/count/first/last (default mean). `partition_by` \
restarts the frame per group. Returns every source column plus \
`<value_col>_rolling_<window>`. Operates on a file `path` or an `open_tab`."
    )]
    async fn rolling_window(
        &self,
        Parameters(p): Parameters<tools::rolling_window::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::rolling_window::handle(self, p).await
    }

    #[tool(
        description = "Compute a pairwise correlation matrix over the numeric columns of a \
tabular file or open tab. `method` is `pearson` (linear, default) or `spearman` (monotonic, \
rank-based). Non-numeric columns are ignored; per pair only rows where both values are present \
are used. Returns `{columns, matrix}` where `matrix[i][j]` correlates `columns[i]` with \
`columns[j]` (null when undefined)."
    )]
    async fn correlation(
        &self,
        Parameters(p): Parameters<tools::correlation::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::correlation::handle(self, p).await
    }

    #[tool(
        description = "Search every tabular file in a directory (one level deep) for a value, \
like grep across files. `query` + `mode` (`plain` default / `wildcard` / `regex`), with optional \
`case_sensitive` and `whole_word`. Skips files larger than `max_file_size_mb` (default 50) and \
unparseable files. Returns `hits` (`{file, row, column, snippet}`), `skipped`, `files_searched`, \
`total_hits`, and `truncated` (capped at `max_results`, default 1000)."
    )]
    async fn grep_files(
        &self,
        Parameters(p): Parameters<tools::grep_files::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::grep_files::handle(self, p).await
    }

    #[tool(
        description = "Rename, cast, or drop columns of a tabular file and write the result back \
(the column-level edit `edit_table` does not do). `rename` is `{from, to}` pairs; `cast` is \
`{column, type}` (Arrow type name) re-typing the column and converting its cells; `drop` is \
column names. Order: drop, then rename, then cast. Writes to `output_path` (default: overwrite \
`path`). Database files are not valid sources or targets."
    )]
    async fn transform_columns(
        &self,
        Parameters(p): Parameters<tools::transform_columns::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::transform_columns::handle(self, p).await
    }

    #[tool(
        description = "Anonymise / mask sensitive columns of a tabular file and write the result. \
`rules` is `{column, strategy}` where strategy is hash / partial_mask / redact / fake; a shared \
`salt` makes output non-guessable and keeps duplicates linked. Null/empty cells pass through. \
Writes to `output_path` (default: overwrite `path`). Database files are not valid sources or \
targets."
    )]
    async fn anonymize(
        &self,
        Parameters(p): Parameters<tools::anonymize::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::anonymize::handle(self, p).await
    }

    #[tool(
        description = "Concatenate (union) multiple tables into one, reconciling differing \
schemas. Each entry in `sources` has a `path` (file) or `open_tab` (GUI tab name / `@active`), \
plus an optional `table` for multi-table sources. By default takes the union of all columns \
(missing cells become null) and widens conflicting numeric types (int+float -> float, any other \
disagreement -> text). Use `drop` to omit column names from the output, and `cast` \
(`[{\"column\": \"name\", \"type\": \"Float64\"}, ...]`) to override a column's target Arrow \
type. `limit` caps response rows (0 = unlimited); `unlimited: true` lifts the 5,000,000-row \
file-loader cap. Requires at least two sources."
    )]
    async fn union_tables(
        &self,
        Parameters(p): Parameters<tools::union::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::union::handle(self, p).await
    }

    #[tool(
        description = "Join N tabular sources left-to-right on shared key columns. Each entry \
in `sources` has a `path` (file) or `open_tab` (GUI tab name / `@active`), plus an optional \
`table` for multi-table sources. Sources are assigned names `t0`, `t1`, ... and joined in \
order using a SQL `USING (on)` clause. `how` sets the join type: `left` (default), `inner`, \
`right`, or `full`. Duplicate key columns are collapsed into one in the output. Requires at \
least two sources and at least one key column in `on`. `limit` caps response rows (0 = \
unlimited); `unlimited: true` lifts the 5,000,000-row file-loader cap."
    )]
    async fn join_tables(
        &self,
        Parameters(p): Parameters<tools::join::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::join::handle(self, p).await
    }

    #[tool(
        description = "Remove duplicate rows from a tabular file or open tab. `on` lists the \
column names whose combined value forms the duplicate key; omit it (or pass an empty list) to \
deduplicate on all columns (whole-row equality). `keep` controls which occurrence to retain: \
`first` (default) keeps the earliest, `last` keeps the latest. Surviving rows are returned in \
original order. Returns the same `{schema, rows, row_count, truncated, ...}` shape as \
`read_table`. `limit` caps response rows (0 = unlimited); `unlimited: true` lifts the \
5,000,000-row file-loader cap."
    )]
    async fn drop_duplicates(
        &self,
        Parameters(p): Parameters<tools::dedupe::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::dedupe::handle(self, p).await
    }

    #[tool(
        description = "Fill missing or empty cells in one column of a tabular file or open \
tab. `column` is the column name to impute; `strategy` chooses the fill method: `mean` or \
`median` (numeric columns only), `mode` (most frequent value), `ffill` (forward-fill from the \
previous non-null row), `bfill` (backward-fill from the next non-null row), or `const` (fill \
with the literal string in `value`). Returns the table with the imputed column, in the same \
`{schema, rows, row_count, truncated, ...}` shape as `read_table`. `limit` caps response rows \
(0 = unlimited); `unlimited: true` lifts the 5,000,000-row file-loader cap."
    )]
    async fn fill_missing(
        &self,
        Parameters(p): Parameters<tools::impute::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::impute::handle(self, p).await
    }

    #[tool(
        description = "Flag numeric outlier cells per column. `method` is `iqr` (default, \
interquartile range: flags values outside [q1 - k*IQR, q3 + k*IQR]) or `zscore` (flags values \
where |z| > k). Default `k` is 1.5 for IQR and 3.0 for z-score; override with `k`. `columns` \
restricts detection to named columns (default: all numeric columns). Columns with fewer than 4 \
numeric values are skipped. Returns `{flagged: [{row, column}, ...], count, method, k}`."
    )]
    async fn detect_outliers(
        &self,
        Parameters(p): Parameters<tools::outliers::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::outliers::handle(self, p).await
    }

    #[tool(
        description = "Detect likely PII columns in a tabular file or open tab. Scans up to \
`sample_rows` rows per column (default 500) and reports columns where more than half the \
non-empty cells match a known PII pattern (email, phone, IBAN, credit card, SSN). Returns \
`{findings: [{column, kind, confidence}, ...], suggested_rules: [...]}` where `suggested_rules` \
are default anonymise-column rules (full SHA-256 hash) ready to pass to the `anonymize` tool."
    )]
    async fn detect_pii(
        &self,
        Parameters(p): Parameters<tools::pii::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::pii::handle(self, p).await
    }

    #[tool(
        description = "Split a table into one file per distinct value of a column, written into \
a directory. Returns the list of files written. `column` is the column name to partition on; \
`out_dir` is the output directory (created if absent); `format` overrides the output extension \
(defaults to the source file's extension, or is required when the source is an open tab without \
a known path). The response is `{ files: [{value, path, rows}, ...], count }`. This is a write \
tool and is unavailable in read-only mode."
    )]
    async fn partition_table(
        &self,
        Parameters(p): Parameters<tools::partition::Params>,
    ) -> Result<CallToolResult, McpError> {
        tools::partition::handle(self, p).await
    }
}

// `router = self.tool_router` tells the macro to dispatch via the pre-built
// router stored on the instance, instead of calling `Self::tool_router()`
// (which would rebuild the route table on every tool call).
#[tool_handler(router = self.tool_router)]
impl ServerHandler for OctaMcpServer {
    fn get_info(&self) -> ServerInfo {
        let row_limit_str = self
            .default_row_limit
            .map_or_else(|| "unlimited".to_string(), |n| n.to_string());
        let cell_cap_str = if self.cell_byte_cap == 0 {
            "unlimited".to_string()
        } else {
            format!("{} bytes", self.cell_byte_cap)
        };
        let instructions = format!(
            "Octa MCP server - inspect tabular data files (Parquet, CSV, JSON, SQLite, DuckDB, \
             Excel, ORC, Arrow, Avro, SAS, SPSS, Stata, RDS, HDF5, NetCDF, DBF, GeoPackage, and \
             text formats) and run DuckDB SQL against them.\n\n\
             Default response row limit: {row_limit_str}. Default cell-size cap: {cell_cap_str}.\n\
             Streaming formats (Parquet, CSV, TSV) load up to 5,000,000 rows by default.\n\
             Parquet files with very many row groups fall back to a DuckDB-backed reader.\n\n\
             Every result-bearing tool exposes:\n\
             - `limit` - caps how many rows the *response* carries (pass 0 for unlimited).\n\
             - `unlimited: true` - also lifts the streaming file-loader cap so the tool sees \
             every row on disk. Use both together to truly return every row.\n\
             Flags `truncated` / `cell_truncated` tell you when re-querying is worthwhile.\n\n\
             Available tools: read_table, tail, sample, schema, list_tables, list_objects, \
             copy_object, move_object, delete_object, list_db_connections, list_db_tables, db_relationships, query_db, \
             sync_sql, write_workbook, write_db_table, copy_db_table, count_rows, run_sql, convert, \
             export_schema, profile, find_duplicates, fuzzy_duplicates, value_frequency, search, \
             compare_schemas, diff_tables, validate_against_schema, describe_file, unique_columns, \
             check_rules, data_drift, schema_drift, create_report, fuzzy_join, suggest_join_keys, \
             diagnose_join, harmonise_schemas, write_table, edit_table, pivot, resample_timeseries, \
             batch_convert, rolling_window, correlation, grep_files, transform_columns, anonymize, \
             union_tables, join_tables, drop_duplicates, fill_missing, detect_outliers, detect_pii, \
             partition_table."
        );
        // NOT `Implementation::from_build_env()`: its `env!` macros expand
        // inside the rmcp crate, so it reports the server as `rmcp` at
        // rmcp's version. Clients show this name to the user.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("octa", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(instructions)
    }
}

/// Run the MCP server over stdio. Blocks until the client disconnects.
/// `default_row_limit` and `cell_byte_cap` come from `AppSettings`;
/// `read_only` (from `--mcp-read-only`) drops the file-writing tools.
pub async fn run(
    default_row_limit: Option<usize>,
    cell_byte_cap: usize,
    read_only: bool,
    allow_schema_changes: bool,
    backup_before_modify: bool,
    large_file_min_bytes: usize,
) -> anyhow::Result<()> {
    let row_str = default_row_limit.map_or_else(|| "unlimited".to_string(), |n| n.to_string());
    let cell_str = if cell_byte_cap == 0 {
        "unlimited".to_string()
    } else {
        format!("{cell_byte_cap} bytes")
    };
    let file_cap = octa::formats::initial_load_rows();
    let file_cap_str = if file_cap == usize::MAX {
        "unlimited".to_string()
    } else {
        format!("{file_cap}")
    };
    let mode_str = if read_only {
        " [read-only: write tools disabled]"
    } else {
        ""
    };
    eprintln!(
        "octa --mcp ready{mode_str} (default response row limit: {row_str}, cell cap: {cell_str}, \
         file-loader cap: {file_cap_str}; override per-call via `limit` / `unlimited`)"
    );
    let server = OctaMcpServer::new(
        default_row_limit,
        cell_byte_cap,
        read_only,
        allow_schema_changes,
        backup_before_modify,
        large_file_min_bytes,
    );
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod read_only_tests;
