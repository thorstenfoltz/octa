---
paths:
  - "src/mcp/**"
  - "tests/mcp_smoke_tests.rs"
  - "docs/mcp/**"
---
# MCP server

`octa --mcp` runs a stdio JSON-RPC server (rmcp 1 + tokio current-thread). `src/mcp/`, one file per tool under `tools/`; **the authoritative tool roster is `tools/mod.rs`, not this file**. `--mcp-read-only` (clap `requires = "mcp"`) threads a `read_only` bool through `run_mcp` -> `mcp::run` -> `OctaMcpServer::new(.., read_only)`, which `tool_router.remove_route`s every name in `WRITE_TOOL_NAMES` (`write_table`/`edit_table`/`convert`/`transform_columns`/`anonymize`/`partition_table`/`batch_convert`/`create_report`/`harmonise_schemas`/`write_db_table`/`copy_db_table`, plus the cloud write/move/delete tools) - a read-only surface for agent frameworks; `mcp::read_only_tests` covers it. The data-ops tools wrap `src/data/{dedupe,impute,outliers,pii,union,join,partition}.rs`; row-returning ones reuse `tools::table_to_json`, analytics ones emit custom `json!`. Most are thin wrappers over pure `src/data/` functions; `profile` wraps `run_query(table, "SUMMARIZE data")`. `pivot`/`correlation`/`grep_files` are read-only analytics (kept under `--mcp-read-only`); `pivot` builds DuckDB PIVOT/UNPIVOT via the shared `octa::data::pivot` (same builders as the GUI dialog), `correlation` wraps `octa::data::correlation`, `grep_files` mirrors the GUI Multi-search directory scope (`octa::data::multi_search` + `read_sorted_dir`). `transform_columns` is the write tool `edit_table` is not: rename/cast/drop columns then write back (order: drop, rename, cast). `validate_against_schema` wraps `compare_schemas` after `parse_json_schema` (Timestamp normalises to `Timestamp(Microsecond, None)` - JSON Schema cannot carry the unit/tz tuple).

- Tool descriptions are string literals at the `#[tool(description = ...)]` site (rmcp's macro rejects a `const`).
- `schemars` pinned to `1` (matches rmcp's re-export; 0.8 breaks `Parameters<T>`).
- `get_info` uses `Implementation::new("octa", env!("CARGO_PKG_VERSION"))`, **not** `Implementation::from_build_env()` — the latter's `env!` macros expand inside the rmcp crate, so the server introduced itself to every client as `rmcp 2.2.0`. `tests/mcp_smoke_tests.rs` asserts `serverInfo.name == "octa"`.
- Caps: `mcp_default_row_limit: Option<usize>` (default `Some(1000)`, `None`=unlimited), `mcp_default_cell_bytes: usize` (default 65536, `0`=no cap). Per-call `limit` `Some(0)`=unlimited; `unlimited: bool` installs an `InitialLoadRowsGuard` in `spawn_blocking` to lift the file cap (`limit` only slices the response). Responses surface `truncated`/`total_rows_available`/`cell_truncated`; oversized cells get a `[truncated: N bytes; ...]` marker.
- Settings read **once at startup**; defaults duplicated literally in `src/ui/settings.rs::{default_mcp_row_limit, default_mcp_cell_bytes}` (settings are library, `src/mcp/` is binary).
- Blocking work on `spawn_blocking`; `run_mcp` installs `tracing_subscriber` to stderr (stdout is JSON-RPC) and prints a ready banner.

Adding a tool: drop `src/mcp/tools/foo.rs` with a `Params` struct + `pub async fn handle(...)`, register in `tools/mod.rs`, add a wrapper method on `OctaMcpServer` with `#[tool(description = "...")]`.

**Cloud URLs (Plan 5).** Read tools accept a cloud URL (`s3://`/`az://`/`gs://`) as `path`: `ToolContext::resolve` detects it via `octa::cloud::parse_cloud_url`, downloads to a temp file (`cloud_fetch_to_temp`, `tmp.keep()` leak), and reads it through the registry. `list_objects` (read-only, kept under `--mcp-read-only`) lists one bucket level by URL. Creds: `ToolContext.cloud_settings: Option<AppSettings>` (None for MCP/CLI -> ephemeral connection + `resolve_ambient_creds`; Some for chat -> matches a saved `cloud_connections` by kind+bucket then `resolve_creds`, resolved lazily on the worker so the aws-CLI shell-out stays off the UI thread). `CloudConnection` carries `allow_writes: bool` (`serde(default)` = **false**, secure by default - existing saved connections need re-opting in after upgrade; checked IN ADDITION to the global `cloud_writes_enabled`, both must allow; GUI gate `OctaApp::cloud_tab_writable` covers save/save_tab/auto-save/upload, chat gate in `resolve_write_dest` after `resolve_cloud`; MCP ephemeral connections default writable, its gate stays `--mcp-read-only`), `prefix: Option<String>` (confine to a key prefix) and `account_level: bool` (browse all buckets/containers; `bucket` empty); `CloudConnection::covers(&CloudLocation)` matches exact bucket / prefix-contained / any-bucket-same-kind. Account-level bucket enumeration is `cloud::list_account_buckets` (CLI shell-out: aws/az/gcloud). The sidebar browser roots a prefix-scoped connection at its prefix and lists buckets for an account-level one (per-bucket provider via `cloud_browser::bind_bucket`, keys re-qualified `<bucket>/<key>`). MCP/chat `resolve_cloud` matches via `covers` and binds an account-level connection to the URL's bucket. Settings cloud form has a scope checkbox + prefix field. Chat sandbox: an unsaved bucket is rejected when `restrict_filesystem`; MCP allows any bucket via ambient creds. Azure ephemeral needs `AZURE_STORAGE_ACCOUNT` (an `az://` URL cannot carry the account). Cloud tools flow into the Assistant automatically via `define_chat_tools!`. **Cloud WRITES**: write tools (`write_table`/`convert`/`transform_columns`/`anonymize`/`run_sql write_to`/`create_chart`/`write_text`) accept a cloud-URL target via `ToolContext::resolve_write_dest` (returns a `WriteDest` = local path OR temp file + provider; `WriteDest::finish()` uploads the temp on success, `is_cloud()` skips local-only steps like backups). Gating mirrors reads: chat needs `cloud_writes_enabled` + a saved connection; MCP uses ambient creds for any bucket. `write_table` to cloud is overwrite-only (the temp has no existing-object semantics). `resolve_write_path` stays the local-path/sandbox helper.

`run_sql` mirrors `--sql`: `extra_tables: Vec<{name, path, table?}>` (name sanitised via `sanitize_sql_name`), `attach: Vec<{alias, path}>`, `write_to: Option<{path, schema?, table, mode, create_schema_if_missing?}>` (response becomes `{kind:"write_back", rows_written, created_schema, target}`). Fresh `SqlWorkspace` per call.

`diff_tables` mirrors `--diff`: `mode: Option<String>` (`set` default / `ordered` / `join`) + `on: Option<Vec<String>>` (join keys). `set` keeps the original `only_in_a`/`only_in_b` + `shared_keys`; `ordered`/`join` add `changed_a`/`changed_b` (the differing rows, parallel order), a `changed` array (`{row_a, row_b, changed_columns}` per pair), and `changed_count`/`unchanged_count`. Dispatches to `octa::data::compare`.

`write_table` / `edit_table` are the data-write tools (distinct from `convert` and `run_sql write_to`). Both reuse the registry `write_file` + inverse-of-`table_to_json` helpers in `tools/mod.rs`: `cell_from_json(value, arrow_type)`, `build_data_table(columns, rows)`. `write_table` takes inline `columns` (name + optional Arrow `type`, default `Utf8`) + array-of-arrays `rows`, writes any *file* format by extension; `mode` is `create`/`overwrite`/`append` (append needs matching column names). DB files rejected (their `write_file` needs `db_meta`). `edit_table` edits an existing file in place: `set` (cells; `col` is index or name), `insert_rows` (`at` defaults to append), `delete_rows` (highest-index-first), then `apply_edits()` + `write_file()` (so SQLite/DuckDB keep diff-based saves). No column changes.

## Advertising a subset (`--mcp-tools` / `--mcp-without`)

`mcp::tool_groups` is the shared catalogue (also used by the GUI assistant's
progressive tool loading, see `.claude/rules/chat.md`): every tool's
`ToolGroup`, an English one-liner for the settings tooltip, and `is_write`.
It lives here, not under `src/app/chat/`, because chat already depends on
`mcp::tools` and the reverse would be a cycle.

- `hidden_tools(only, without)` turns the two flag lists into the names to
  drop. Each item is a group id or a tool name; an unrecognised word is an
  `Err` naming it plus every valid group, and `run_mcp` exits on it **before**
  starting a server, since a server quietly advertising the wrong surface is
  the one outcome worth refusing.
- `OctaMcpServer::new(..., hidden)` calls `tool_router.remove_route` for each,
  after the read-only pass, so the two stack and read-only always wins.
- `write_tool_names()` (from `is_write`) is now the **single** write list:
  `--mcp-read-only` and the chat profile's **Allow writes** both read it, so
  the two surfaces cannot drift. It used to be a hand-kept list in each.
- An unadvertised tool is also uncallable: `remove_route` takes it out of
  dispatch, not just `tools/list`.

There is deliberately **no** MCP equivalent of the chat's `enable_tools`
fetching: the client reads `tools/list` once and decides what reaches its
model, so a static filter is the lever that actually works there. Do not add a
settings key for this either - the flags live in the client's own server config
(`claude_desktop_config.json`, `.mcp.json`), which is where a per-client
surface belongs.

Tests: `mcp::tool_groups::tests` (selector parsing), `mcp::read_only_tests`
(router shape), and `tests/mcp_smoke_tests.rs` over the wire - the advertised
list, a refused call to a filtered-out tool, and the startup refusal on a typo.
