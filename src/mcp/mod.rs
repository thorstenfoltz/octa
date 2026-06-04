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
    /// rmcp tool routing table (populated by `#[tool_router]`).
    pub tool_router: ToolRouter<OctaMcpServer>,
}

impl OctaMcpServer {
    /// Build a [`tools::ToolContext`] for one tool call. The MCP server has no
    /// open GUI tabs, so the context carries only the configured caps; the
    /// in-GUI chat agent builds a context with tab snapshots instead. Sharing
    /// the type lets both surfaces call the same `tools::<name>::run`.
    pub fn tool_context(&self) -> tools::ToolContext {
        tools::ToolContext::for_mcp(self.default_row_limit, self.cell_byte_cap)
    }
}

#[tool_router]
impl OctaMcpServer {
    pub fn new(default_row_limit: Option<usize>, cell_byte_cap: usize) -> Self {
        Self {
            default_row_limit,
            cell_byte_cap,
            tool_router: Self::tool_router(),
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
        description = "Count rows in a tabular file. Loads the table and reports its row count. \
For streaming formats (Parquet, CSV, TSV) the count is bounded by Octa's 5,000,000-row \
initial-load cap; the response flags `initial_load_capped: true` when the count may not \
reflect every row in the source. Pass `unlimited: true` to lift the cap and get the true \
total."
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
             Available tools: read_table, tail, sample, schema, list_tables, count_rows, \
             run_sql, convert, export_schema, profile, find_duplicates, value_frequency, \
             search, compare_schemas, diff_tables, validate_against_schema, describe_file, \
             unique_columns."
        );
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(instructions)
    }
}

/// Run the MCP server over stdio. Blocks until the client disconnects.
/// `default_row_limit` and `cell_byte_cap` come from `AppSettings`.
pub async fn run(default_row_limit: Option<usize>, cell_byte_cap: usize) -> anyhow::Result<()> {
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
    eprintln!(
        "octa --mcp ready (default response row limit: {row_str}, cell cap: {cell_str}, \
         file-loader cap: {file_cap_str}; override per-call via `limit` / `unlimited`)"
    );
    let server = OctaMcpServer::new(default_row_limit, cell_byte_cap);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
