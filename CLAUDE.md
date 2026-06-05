# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Commands

```bash
cargo build              # debug build
cargo build --release    # release build
cargo run -- file.parquet
cargo test
cargo test <test_name>
cargo clippy
cargo fmt
```

## Installation

`sudo ./install.sh` for system-wide on Linux, `./install.sh ~/.local` for user-local. `install.bat` on Windows. Linux build needs `libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libfontconfig1-dev libfreetype6-dev`.

## CI Pipeline

PRs to `master` run three jobs: `test` (fmt + clippy + cargo test, one shared `Swatinem/rust-cache@v2` target dir), `licenses` (`cargo deny check licenses`), and `megalinter` (shell/security/markdown via `ghcr.io/oxsecurity/megalinter-rust:v9`). Clippy/rustfmt live in `test`, not megalinter (no Rust cache there, ~1000s/PR).

`release.yml` (`workflow_dispatch`) intentionally does **not** re-run `cargo test` (PR CI already validated the merged commit). Jobs: `build-linux` (+ AppImage), `build-windows`, `build-macos` (aarch64), `build-macos-x86` (Intel, `macos-13`, `x86_64-apple-darwin` tarball), `publish` (needs all builds), `aur-publish`, and `docker-publish` (runs **in parallel** with the builds, no `needs:` — the Dockerfile builds from source so it never depended on the release artifacts).

## CLI

`octa` is **flag-driven**, not subcommand-driven: pick one of `--schema`, `--head`, `--tail`, `--sample`, `--convert`, `--sql`, `--export-schema` (`-e`), `--compare-schemas`, `--diff`, `--describe`, `--validate-schema`, `--unique-columns`, `--mcp` (mutually exclusive via clap `group = "action"`); with none, launches the GUI with the positional file list. Implemented in `src/cli/`, one file per action. Global `-f / --format {tsv|json|csv}` (default tsv) routes through `src/cli/output.rs`; `--convert` and `--export-schema` ignore it. `--export-schema` picks a target via `-t / --target` (`SchemaTargetArg` → `octa::data::schema_export::SchemaTarget`). `disable_help_flag = true` + a custom `--help` (`ArgAction::HelpLong`) makes `-h`/`--help` identical.

`--validate-schema` returns exit `1` on a successful read where schemas drift (CI gating); `cli::dispatch` pulls it out of the normal `Result → ExitCode` mapping so `validate_schema::run` returns `Result<ExitCode, _>`. `--mcp` does **not** route through `cli::dispatch`: `main.rs::run_mcp` peels it off first so GUI/CLI paths never build a tokio runtime.

Man page source `docs/cli/octa.1.adoc`; the release renders/bundles `octa.1` (install.sh falls back to rendering); mkdocs mirrors it at `docs/cli/man-page.md`.

Global `--rows N|all` overrides the streaming initial-load cap for one run (`cli::parse_rows_flag`; `all` → `usize::MAX`); `cli::dispatch` installs an `octa::formats::InitialLoadRowsGuard`.

`--sql` workspace flags: `--sql-table NAME=PATH` (repeatable) adds workspace tables; `--sql-attach ALIAS=PATH` ATTACHes a DuckDB/SQLite file (`alias.schema.tbl`); `--sql-write-to PATH` + `--sql-write-table TABLE` (+ optional `--sql-write-schema`, `--sql-write-mode {create|append|replace}`) persists the SELECT instead of printing. `src/cli/sql.rs` builds + tears down a fresh `SqlWorkspace` per invocation.

Adding an action: append a flag to `cli::Cli`, add an `Action` variant, drop `src/cli/<verb>.rs`, extend `Cli::detect_action`, add a `cli::dispatch` arm.

## MCP server

`octa --mcp` runs a stdio JSON-RPC server (rmcp 1 + tokio current-thread). `src/mcp/`, one file per tool under `tools/`. Tools: `read_table`, `tail`, `sample`, `schema`, `list_tables`, `count_rows`, `run_sql`, `convert`, `export_schema`, `profile`, `find_duplicates`, `value_frequency`, `search`, `compare_schemas`, `diff_tables`, `describe_file`, `validate_against_schema`, `unique_columns`, `write_table`, `edit_table`. Most are thin wrappers over pure `src/data/` functions; `profile` wraps `run_query(table, "SUMMARIZE data")`; row-returning tools reuse `tools::table_to_json`. `validate_against_schema` wraps `compare_schemas` after `parse_json_schema` (Timestamp normalises to `Timestamp(Microsecond, None)` — JSON Schema can't carry the unit/tz tuple).

- Tool descriptions are string literals at the `#[tool(description = ...)]` site (rmcp's macro rejects a `const`).
- `schemars` pinned to `1` (matches rmcp's re-export; 0.8 breaks `Parameters<T>`).
- Caps: `mcp_default_row_limit: Option<usize>` (default `Some(1000)`, `None`=unlimited), `mcp_default_cell_bytes: usize` (default 65536, `0`=no cap). Per-call `limit` `Some(0)`=unlimited; `unlimited: bool` installs an `InitialLoadRowsGuard` in `spawn_blocking` to lift the file cap (`limit` only slices the response). Responses surface `truncated`/`total_rows_available`/`cell_truncated`; oversized cells get a `[truncated: N bytes; ...]` marker.
- Settings read **once at startup**; defaults duplicated literally in `src/ui/settings.rs::{default_mcp_row_limit, default_mcp_cell_bytes}` (settings are library, `src/mcp/` is binary).
- Blocking work on `spawn_blocking`; `run_mcp` installs `tracing_subscriber` to stderr (stdout is JSON-RPC) and prints a ready banner.

Adding a tool: drop `src/mcp/tools/foo.rs` with a `Params` struct + `pub async fn handle(...)`, register in `tools/mod.rs`, add a wrapper method on `OctaMcpServer` with `#[tool(description = "...")]`.

`run_sql` mirrors `--sql`: `extra_tables: Vec<{name, path, table?}>` (name sanitised via `sanitize_sql_name`), `attach: Vec<{alias, path}>`, `write_to: Option<{path, schema?, table, mode, create_schema_if_missing?}>` (response becomes `{kind:"write_back", rows_written, created_schema, target}`). Fresh `SqlWorkspace` per call.

`write_table` / `edit_table` are the data-write tools (distinct from `convert` and `run_sql write_to`). Both reuse the registry `write_file` + inverse-of-`table_to_json` helpers in `tools/mod.rs`: `cell_from_json(value, arrow_type)`, `build_data_table(columns, rows)`. `write_table` takes inline `columns` (name + optional Arrow `type`, default `Utf8`) + array-of-arrays `rows`, writes any *file* format by extension; `mode` is `create`/`overwrite`/`append` (append needs matching column names). DB files rejected (their `write_file` needs `db_meta`). `edit_table` edits an existing file in place: `set` (cells; `col` is index or name), `insert_rows` (`at` defaults to append), `delete_rows` (highest-index-first), then `apply_edits()` + `write_file()` (so SQLite/DuckDB keep diff-based saves). No column changes.

## Chatbot (in-GUI assistant)

A docked chat panel (`src/app/chat_panel.rs`, app-level like SQL) where an LLM drives Octa's tools in an agentic loop over the open tabs/files. Toggle via toolbar **Analyse -> Assistant**, **View** menu, or `ShortcutAction::ToggleChatPanel` (default `Ctrl+Shift+A`); header has a close [x]. GUI-only.

Core reuse lever: each MCP tool's work lives in `pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value>` (`src/mcp/tools/<name>.rs`); the MCP `handle` is a thin `server.tool_context()` -> `spawn_blocking(run)` wrapper, and the chat layer calls the **same** `run`. `ToolContext`/`Source`/`TableSnapshot`/`source_from` live in `src/mcp/tools/mod.rs`; `resolve(&Source)` reads via `read_with_registry` or clones an open-tab snapshot. Single-source tools' `Params` carry `open_tab: Option<String>` (`"@active"`, a tab name, or absent->`path`); compare/diff carry `open_tab_a`/`open_tab_b`; `write_table`/`edit_table` reject an `open_tab` target. `path` fields are `#[serde(default)]`.

**Filesystem sandbox (chat only).** `ToolContext` carries `restrict_filesystem` + `allowed_read_paths` (canonical open-tab paths) + `export_dir`; `for_mcp` leaves them off. `build_tool_context` (`chat_panel.rs`) sets them for chat: reads confined to open tabs (`ensure_readable`); a multi-table file is fully reachable once any sheet is open. Writes go through `resolve_write_path` (bare name -> `export_dir`; absolute path honoured). `TableSnapshot.handle` (`#1`...) addresses same-named tabs; `snapshot_for_pathish(s)` matches handle/name/filename (gated on `table.is_none()` so sibling-sheet reads still hit disk). `run_sql.extra_tables` resolve open tabs (`add_table` + `TableOrigin::TabClone`); `run_sql.write_to` also writes plain file formats. Chat-only tool `create_chart` (`src/mcp/tools/create_chart.rs`, in `define_chat_tools!`, no MCP `handle`) reuses `octa::data::chart::build_chart` + `chart_export`.

**Non-tabular files.** Text/code/Markdown files load as a single `Line` column. Chat-only tools `read_text` (joins the `Line` column into the file's text — captures in-memory edits) and `write_text` (writes content to a new file in `export_dir`, an absolute path, or back to an open tab's file on disk via `open_tab`; the live editor does **not** refresh — the user reloads). Both live in `src/mcp/tools/{read_text,write_text}.rs` (no MCP `handle`). The system prompt (`src/app/chat/mod.rs::build_system_prompt`) tells the model to use these for prose/code.

Module layout under `src/app/chat/`:
- `types.rs` - `Role` / `ContentBlock {Text|ToolUse|ToolResult}` / `Message` / `ChatEvent` / `StopReason` / `ToolDef`.
- `tools.rs` - `define_chat_tools!` builds **both** `tool_defs()` (schemas from `schemars::schema_for!(Params)`, `$schema`/`title` stripped) and `dispatch(ctx, name, args)` from one list. Descriptions come from a `pub const DESCRIPTION` per tool module.
- `providers/` - one file per backend (`anthropic`, `openai`, `openai_compat`/`ollama` reusing openai's wire helpers, `gemini`) behind `trait ChatProvider`. `ProviderConfig.max_tokens: Option<usize>` (None=omit field; Anthropic substitutes a high value). OpenAI emits `max_completion_tokens` (`build_body` takes a `token_field` so compat/ollama keep `max_tokens`). `gemini.rs::to_gemini_schema` sanitises schemars JSON Schema to Gemini's OpenAPI subset (resolve `$ref`, `Option`/`anyOf`->`nullable`, drop `additionalProperties`/`$schema`/`$defs`/`default`/`title`) else Gemini 400s. `stream_sse` extracts `error.message` from non-2xx bodies. Ollama is first-class: keyless `ChatProviderKind::Ollama`, `chat_ollama_url`, `src/app/chat/ollama.rs` (`is_running`/`list_models` via `/api/tags`; `start_server` spawns `ollama serve` in its own process group via `pre_exec(setsid)` so `stop_child_group` kills server + `llama-server` child; `stop_local_server` pkills for the unowned case; needs the `libc` unix dep). Panel worker threads do the model dropdown + Refresh/Start/Stop + ~5s status poll; Octa-started server killed on `OctaApp::on_exit`. SSE over blocking `ureq` (`json`) on a `std::thread`; `stream_sse` configures an `Agent` and uses `into_reader()` (not `read_to_string`, which defeats streaming). Providers assemble tool calls fully before emitting `ChatEvent::ToolCall`.
- `session.rs` - `ChatSessionState` (messages, `streaming`, `running`/`cancel` `Arc<AtomicBool>`, error) behind `Arc<Mutex<..>>`.
- `agent.rs` - `spawn_turn` runs on a `std::thread` holding only a cloned `egui::Context` + `Arc<Mutex<state>>` + a moved `ToolContext` (never borrows `TabState`). Loops up to `chat_max_tool_iterations`: stream -> commit -> dispatch tool calls (capped by `truncate_for_model`, 100 KB) -> append -> repeat. Per-turn `cancel`/`running` are fresh `Arc`s installed by `send_chat_message`; `cancel_chat` flips cancel and clears state; the worker's finally only clears shared state when `Arc::ptr_eq` confirms it's still the active turn.
- `persist.rs` - one pretty JSON per session under `<config_dir>/chat_sessions/`; autosaved per turn (debounced) and on new/load.
- `secrets` - in the library at `src/ui/settings/secrets.rs` (re-exported `crate::app::chat::secrets`). Per-provider keys, precedence **env -> keyring -> plaintext settings.toml**; `set_api_key` reports keyring-vs-fallback, `storage_location` reports the source. **Linux keyring = freedesktop Secret Service** (`async-secret-service` + `crypto-rust`, pure-Rust zbus).

Settings live in the **main Settings dialog** (`src/ui/settings/dialog.rs`, "Chat / Assistant" `CollapsingHeader`); the panel's Settings button opens it via `settings_dialog.focus_chat_section`. `chat_*` fields in `src/ui/settings/mod.rs`: `chat_provider`, `chat_models: BTreeMap` per-provider, `chat_base_url`, `chat_ollama_url`, `chat_panel_position`, `chat_temperature` (default **0.0**), `chat_max_tool_iterations`, `chat_max_tokens` (default 16384) + `chat_max_tokens_unlimited`, `chat_export_dir` (default ~/Downloads), `chat_api_keys: BTreeMap`. Numeric settings use comma-tolerant text buffers. Chat tool context caps: 200 rows, 4 KiB/cell. Chat UI strings under `[chat]` in every locale. The **Clear API key** button arms a confirmation (`chat_key_clear_confirm: Option<ChatProviderKind>`) — a second explicit click calls `secrets::delete_api_key`.

**Provider/model presets** are a hand-editable runtime `models.toml` beside `settings.toml` (`src/ui/settings/chat_models.rs`): `preset_models(kind)` / `default_model(kind)` read it, seeded from the `ChatProviderKind` consts and auto-written on first run; `reload()` re-reads (wired to a Settings "Reload models.toml" button). Missing/empty entries fall back to the built-in consts.

Adding a provider: drop `src/app/chat/providers/<name>.rs` impl `ChatProvider`, add a `ChatProviderKind` variant + `make_provider` arm. Adding a tool: free once the MCP tool has `run` + `DESCRIPTION` and is in `define_chat_tools!`.

## Chatbot (in-GUI assistant)

A docked chat panel (`src/app/chat_panel.rs`, app-level like the SQL panel) where the user chats with an LLM that drives Octa's existing tools in an agentic loop against the open tabs and files. Toggle via the toolbar **Analyse -> Assistant**, **View** menu, or `ShortcutAction::ToggleChatPanel` (default `Ctrl+Shift+A`); the panel header has an explicit close [x]. GUI-only (no CLI/man-page surface).

Core reuse lever: each MCP tool's real work was hoisted into `pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value>` in `src/mcp/tools/<name>.rs`; the async MCP `handle` is now a thin `server.tool_context()` -> `spawn_blocking(run)` wrapper, and the chat layer calls the **same** `run`. `ToolContext` / `Source` / `TableSnapshot` / `source_from` live in `src/mcp/tools/mod.rs`: `resolve(&Source)` reads a file via `read_with_registry` or clones an open-tab snapshot. Every single-source tool's `Params` grew `open_tab: Option<String>` (`"@active"` -> active tab, any other name -> that tab, absent -> `path`); two-source tools (compare/diff) grew `open_tab_a`/`open_tab_b`; `write_table`/`edit_table` reject an `open_tab` target in v1 ("writing back to an open tab is not supported yet"). `path` fields are now `#[serde(default)]` so a tab-targeted call needn't pass one.

**Filesystem sandbox (chat only).** `ToolContext` carries `restrict_filesystem` + `allowed_read_paths` (canonical source paths of the open tabs) + `export_dir`. `for_mcp` leaves these off (MCP/CLI are trusted, unchanged). For the chat agent `build_tool_context` (`chat_panel.rs`) sets them: reads are confined to open tabs (`ensure_readable`), so the agent cannot open arbitrary disk files; a multi-table file is fully reachable (`list_tables`/`read_table path+table`) once any one sheet/table is open. Writes go through `resolve_write_path` (bare name -> `export_dir`; absolute path honored). `TableSnapshot.handle` (`#1`, `#2`, ...) makes same-named tabs addressable; `snapshot_for_pathish(s)` matches a handle / display name / filename so the model can target an open tab even when it puts the reference in `path` instead of `open_tab` (gated on `table.is_none()` so sibling-sheet reads still hit disk). `run_sql.extra_tables` resolve open tabs the same way (`add_table` + `TableOrigin::TabClone`) so N open tabs can be JOINed; `run_sql.write_to` now also writes plain file formats (csv/parquet/...) via the registry, not just DuckDB/SQLite. Chat-only tool `create_chart` (`src/mcp/tools/create_chart.rs`, in `define_chat_tools!` but no MCP `handle`) reuses `octa::data::chart::build_chart` + `chart_export` to write png/pdf/svg into `export_dir`.

Module layout under `src/app/chat/`:
- `types.rs` - neutral `Role` / `ContentBlock {Text|ToolUse|ToolResult}` / `Message` / `ChatEvent` / `StopReason` / `ToolDef` (all Serde where persisted).
- `tools.rs` - the `define_chat_tools!` macro builds **both** `tool_defs()` (LLM schemas from `schemars::schema_for!(Params)`, `$schema`/`title` stripped) and `dispatch(ctx, name, args)` from one list, so they never drift. Tool descriptions come from a `pub const DESCRIPTION` per tool module (the MCP `#[tool(description=...)]` literal stays separate because rmcp's macro needs a literal).
- `providers/` - one file per backend (`anthropic`, `openai`, `openai_compat` and `ollama` both reusing openai's wire helpers, `gemini`) behind `trait ChatProvider`. `ProviderConfig.max_tokens: Option<usize>` (None = unlimited -> field omitted; Anthropic requires it, so it substitutes a high value). OpenAI emits `max_completion_tokens`; `run_openai`/`build_body` take a `token_field` arg so compat/ollama keep `max_tokens`. `gemini.rs::to_gemini_schema` sanitises each tool's schemars JSON Schema to Gemini's OpenAPI subset (resolve `$ref`, collapse `Option`/`anyOf` -> `nullable`, drop `additionalProperties`/`$schema`/`$defs`/`default`/`title`) - without it Gemini 400s. `stream_sse` extracts `error.message` from non-2xx JSON bodies for a clean error. `ollama` is first-class: `ChatProviderKind::Ollama` (keyless), `chat_ollama_url`, `src/app/chat/ollama.rs` (`is_running`/`list_models` via `/api/tags`; `start_server` spawns `ollama serve` in its own process group via `pre_exec(setsid)` so `stop_child_group` can kill the whole group - server + its `llama-server` model-runner child, else the runner is orphaned and leaks RAM; `stop_local_server` pkills serve+runner for the unowned case; needs the `libc` unix dep). Panel worker threads do model dropdown + Refresh/Start/**Stop** + a ~5s status poll; the Octa-started server is killed on `OctaApp::on_exit`. SSE over the blocking `ureq` (`json` feature) on a `std::thread`; `stream_sse` in `providers/mod.rs` configures via an `Agent` (a per-request `.config().build()` erases ureq's `WithBody` type-state and drops `send_json`) and uses `into_reader()` (never `read_to_string`, which would buffer and defeat streaming). Providers assemble tool calls in full before emitting `ChatEvent::ToolCall` (the agent never sees arg fragments).
- `session.rs` - `ChatSessionState` (messages, `streaming`, `running`/`cancel` `Arc<AtomicBool>`, error) behind `Arc<Mutex<..>>`.
- `agent.rs` - `spawn_turn` runs the loop on a `std::thread` holding only the cloned `egui::Context` + `Arc<Mutex<state>>` + a moved `ToolContext` of table snapshots (never borrows `TabState`). Loops up to `chat_max_tool_iterations`: stream -> commit assistant turn -> if tool calls, `tools::dispatch` each (capped by `truncate_for_model`, 100 KB) -> append results -> repeat. Per-turn `cancel`/`running` are **fresh `Arc`s** installed by `send_chat_message` (so a cancelled+blocked worker can't resurrect into a later turn); `cancel_chat` flips cancel and clears `running`/`streaming` for an instant-responsive UI; the worker's finally only clears shared state when `Arc::ptr_eq` confirms it is still the active turn. `cancel` polled between SSE lines and tool calls.
- `persist.rs` - one pretty JSON per session under `<config_dir>/chat_sessions/`; autosaved per completed turn (debounced on message count in `chat_panel.rs::autosave_chat_session`) and on new/load.
- `secrets` - **moved to the library** at `src/ui/settings/secrets.rs` (re-exported as `crate::app::chat::secrets`) so the library-side Settings dialog can manage keys. Per-provider keys, precedence **env -> keyring -> plaintext settings.toml**; `set_api_key` returns whether it hit the keyring or fell back, `storage_location` reports the source. `keyring` is a per-OS dep; **Linux uses the freedesktop Secret Service** (`async-secret-service` + `crypto-rust`, pure-Rust zbus, persistent across reboots, no libdbus C dep).

Settings live in the **main Settings dialog** (`src/ui/settings/dialog.rs`, a "Chat / Assistant" `CollapsingHeader`); the panel's Settings button opens it via `settings_dialog.focus_chat_section`. The standalone chat-settings window was removed. `chat_*` fields in `src/ui/settings/mod.rs`: `chat_provider`, `chat_models: BTreeMap` per-provider, `chat_base_url`, `chat_ollama_url`, `chat_panel_position`, `chat_temperature` (default **0.0**), `chat_max_tool_iterations`, `chat_max_tokens` (default 16384) + `chat_max_tokens_unlimited`, `chat_export_dir` (default ~/Downloads), `chat_api_keys: BTreeMap` plaintext fallback. (`chat_enabled` and `chat_include_all_tabs` were removed - the assistant is always available and always sees all open tabs.) Numeric settings use comma-tolerant text buffers, not `DragValue`/`Slider`, and the dialog sets `interaction.selectable_labels=false` so label hover shows the default cursor. The chat tool context uses conservative caps (200 rows, 4 KiB/cell). Chat UI strings live under `[chat]` in every locale (fully translated, 12 langs).

Adding a provider: drop `src/app/chat/providers/<name>.rs` implementing `ChatProvider`, add a `ChatProviderKind` variant + `make_provider` arm. Adding a tool: it comes for free once the MCP tool has a `run` + `DESCRIPTION` and is listed in `define_chat_tools!`.

## Performance settings

- `initial_load_rows` (default **5M**) caps the first streaming batch. Process-wide `AtomicUsize` (`src/formats/mod.rs::INITIAL_LOAD_ROWS`); `set_initial_load_rows` runs from `OctaApp::new`, Settings apply, and `run_mcp`. `InitialLoadRowsGuard` is an RAII override for CLI `--rows` / MCP `unlimited` (safe: CLI single-threaded, MCP current-thread runtime). `initial_load_rows_unlimited` overrides to `usize::MAX`.
- `syntax_highlight_max_bytes` (default 1024 KB) gates the raw editor's syntect pass; Settings exposes a Bytes/KB/MB unit picker.
- `text_mode_extensions: Vec<String>` forces unknown extensions through `TextReader` (checked in `load_file` before registry lookup; lowercased, dot-stripped; unioned into the picker's "All Supported").
- `thousands_separators_in_cells` (default **on**) + `number_separator_style: SeparatorStyle` (English/European) group numeric cells display-only via `src/ui/table_view/rows.rs` -> `octa::data::num_format::format_cell_number` (never touches saved/exported/CLI/MCP output).
- `excel_max_auto_sheets` (default **5**) caps auto-opened Excel sheets; above it `load_file` raises `pending_sheet_picker`.
- `trim_whitespace_on_load` + `warn_on_whitespace_trim` (both default **on**) drive the load-time whitespace pass; `offer_repair_on_malformed` (default **off**) the repair prompt; `language` (default `"en"`) drives `octa::i18n`.

## Licensing

MIT. The `licenses` CI job enforces every transitive license against `deny.toml`'s allowlist; copyleft (AGPL/GPL/LGPL/SSPL) excluded. Artifacts: `THIRD_PARTY_LICENSES.md` (per-crate index, `cargo about generate about.hbs --output-file THIRD_PARTY_LICENSES.md`) and `licenses/<SPDX-id>.txt` (canonical text per identifier, hand-curated — add a file when a new license family enters the tree). `install.sh`/`install.bat` ship all three alongside the binary.

## Architecture

Native desktop GUI for tabular data, `eframe`/`egui` (immediate-mode), Rust edition 2024. Needs a C compiler (duckdb, rusqlite). The crate is both a library (`src/lib.rs` re-exports `data`, `formats`, `ui`) and a binary (`src/main.rs`); tests `use octa::data::*`.

Canonical format registration order: `FormatRegistry::new()` (`src/formats/mod.rs`); user-facing list `docs/getting-started/supported-formats.md` — both update together. Read-only formats (SAS, R, HDF5, NetCDF, EPUB, GeoJSON, Fixed-Width) inherit `supports_write=false`.

`TextReader` claims every highlightable source/config extension (`.py`/`.rs`/`.go`/web markup/...) so they're advertised as supported; `highlight_whitelist_is_supported` pins `HIGHLIGHT_WHITELIST` to the registry's set. `src/formats/sniff.rs::sniff_format` does content-based detection (magic bytes + JSON/CSV probes) for missing/wrong extensions: `reader_for_path` consults it before the Text fallback, `load_file` retries via `try_content_sniff_reload`; `reader_by_name` maps a sniff result to a reader.

### Module Layout

- `src/main.rs`: app entry + root state (`OctaApp`); top-level UI orchestration.
- `src/data/mod.rs`: `DataTable`, `CellValue`, `ColumnInfo`. Cell edits live in a `HashMap<(row,col), CellValue>` overlay; `rows` only mutate on `apply_edits()`. `structural_changes` flags non-cell edits. `db_meta: Option<DbRowMeta>` carries DB row identity (`table_name`, `row_tags: Vec<Option<i64>>`, `original`, `original_columns`); new rows have `None` tags.
- `src/data/search.rs`: `RowMatcher` (Plain/Wildcard/Regex search-filter-replace).
- `src/data/encoding.rs`: `decode_bytes(&[u8]) -> (String, &'static str)` and `read_to_string_detected(path)` — BOM sniff, then UTF-8 fast path, then `chardetng` + `encoding_rs`. Used by `TextReader`/`MarkdownReader` so non-UTF-8 (Windows-1252/Latin-1/UTF-16) text opens. CSV keeps its own streaming path (`csv_reader` `lossy_utf8`).
- `src/formats/mod.rs`: `FormatReader` trait + `FormatRegistry`. Multi-table sources implement `list_tables`/`read_table(name)`; single-table formats fall through to `read_file`.
- `src/formats/parquet_reader.rs`: `build_arrow_array` + `data_type_from_string` map our type-name strings <-> Arrow `DataType` (update together). Native reader (parquet-58) rejects >32,767 row groups; `read_file` retries via a DuckDB path (`read_parquet` + `query_arrow`). Both honour `initial_load_rows` and strip pandas index columns via `pandas_index_columns`.
- `src/formats/sqlite_reader.rs`: `rusqlite` (bundled); `rowid` in `db_meta.row_tags`; diff writes; schema changes rejected.
- `src/formats/duckdb_reader.rs`: `duckdb` (bundled); synthetic `__octa_row_id BIGINT` (stripped from user schema); diff-on-save.
- `src/formats/spss_reader.rs`: SPSS read-only via `ambers` (needs arrow ^57; pinned `arrow57 = { package = "arrow", version = "57" }` alongside arrow 58).
- `src/formats/{sas,stata,rds,dbf,hdf5,netcdf,geojson,epub}_reader.rs`: pure-Rust, mostly read-only. RDS = tabular subset. `geojson_reader` claims only `.geojson`. `epub_reader` uses `rbook` + `htmd` (Apache-2.0; `epub`/`html2md` crates are GPL-3.0, blocked).
- `src/formats/fwf_reader.rs`: fixed-width (`.fwf`/`.prn`), read-only. `infer_field_ranges` finds boundaries from always-blank char positions; first line is header. All Utf8. Pure `read_fwf(&str, &Path)`.
- `src/sql/mod.rs`: re-exports `SqlWorkspace`, `AttachKind`, `WriteTarget`, `WriteMode`, `WriteReport`, `QueryKind`, `QueryOutcome`, `sanitize_sql_name`. `run_query(table, query)` is a one-shot wrapper (fresh workspace, registers `data`, tears down).
- `src/sql/workspace.rs`: `SqlWorkspace` owns one persistent DuckDB conn + `tables` + `attachments`. API: `new`, `set_active_table` (the `data` TEMP TABLE), `add_table_from_{file,datatable}`, `attach`, `detach`, `remove_table`, `list_attached_tables`, `execute`, `write_result_to_db`. DuckDB ATTACH native for `.duckdb`/`.ddb`; SQLite tries `INSTALL/LOAD sqlite` then falls back to per-table `SqliteReader` (`alias__table`); SQLite writes always via `rusqlite`. `execute` columns all `Utf8`. Sync; MCP wraps with `spawn_blocking`.
- `src/ui/table_view.rs`: virtual renderer (scroll, resize min 60px, drag-reorder, multi-cell/row/column selection, inline edit, context menu, line breaks, binary display). Returns an `Interaction` struct; caller mutates.
- `src/ui/toolbar.rs`: toolbar (logo, File/Edit/View/Search/Help menus, zoom, view-mode radio, custom title-bar buttons), wrapped in `egui::MenuBar`. Returns `ToolbarAction`.
- `src/ui/directory_tree.rs`: sidebar browser, left/right per setting; each row a full-width `Sense::click()` rect; `read_sorted_dir` is the sole disk walker.
- `src/ui/table_picker.rs`: modal for multi-table sources (single-table DBs auto-load). Window id `octa_table_picker_dialog_v2`; height fit-to-content up to `table_picker_visible_rows` (default 10); body/footer via `Panel::bottom` + `CentralPanel`.
- `src/view_modes/`: per-mode renderers (Notebook, Markdown, Raw, JSON/YAML Tree, SQL, Compare, EPUB, Map). JSON/YAML trees share `render_value_tree` (private `TreeKind`); `markdown::render_pulldown` is `pub(crate)` for EPUB reuse. Markdown Edit/Split editor (`render_editor_pane`) has a line-number gutter mirroring the Raw view. Notebook **source cells are editable** (`render_notebook_view` takes `&mut TabState` + `readonly`); frameless `TextEdit::multiline`, code cells keep syntect via `.layouter`; writes through `tab.table.set`. `.ipynb` writer (`jupyter_reader::write_notebook`) re-reads original via `source_path` and overwrites only `source`/`cell_type`, preserving outputs/metadata.
- `src/view_modes/map.rs`: GeoJSON Map via `walkers` (own tokio runtime in a private thread). Polygon holes stroked, not cut (egui has no even-odd rule).
- `src/view_modes/text_ops.rs`: `apply_case_to_selection` (Upper/Lower) shared by SQL/raw editors (char-to-byte range mapping).
- `src/ui/syntax.rs`: syntect over `SyntaxSet::load_defaults_newlines()` + bundled `assets/Terraform.sublime-syntax`. `HIGHLIGHT_WHITELIST` excludes formats with dedicated views (JSON/YAML/XML/Markdown/TOML/CSV/TSV). Size-gated by `syntax_highlight_max_bytes`.
- `src/ui/settings.rs` (+ `settings/`): `AppSettings` + TOML persistence (`~/.config/octa/settings.toml`, `~/Library/Application Support/Octa/`, `%APPDATA%\Octa\`). `IconVariant::Random` resolves once into `OctaApp.resolved_icon`. Rainbow easter-egg theme: `ensure_logo_textures` renders `assets/octa-random.svg` while active; leaving Rainbow nulls the logo textures so the configured icon returns.
- `src/ui/status_bar.rs`: bottom bar; nav input jumps to `R5:C3`/`R5`/`C3`/row#/column name. Takes `busy` + `busy_hint` for a Spinner.
- `src/ui/theme.rs`: `ThemeMode`, `ThemeColors`; `apply_theme(ctx, mode, font_size)` is the single entry point.

### Key Design Patterns

- **Interaction structs**: UI components return plain structs; `main.rs` reads them and mutates state. No callbacks.
- **Edit overlay**: cell edits in `DataTable.edits` until `apply_edits()`; structural mutations shift edit indices atomically.
- **Lazy filter**: `filter_dirty` triggers `recompute_filter()`; `filtered_rows: Vec<usize>` maps display->data index.
- **Selection model**: cell click selects one; **Ctrl+click** toggles into a disjoint `selected_cells` set (first toggle promotes the prior single cell). Row-number click selects a row (Ctrl/Shift extends); header click selects a column. Copy honours the selection.
- **Tab multi-selection for Compare**: Ctrl-click a non-active tab toggles `OctaApp.tab_multi_selection`; `CompareSelectedTabs` (F9) runs with exactly one selected; `begin_compare_with_tab` clones the right side from memory (in-memory edits carry through).
- **Right-click copy**: TextEdit views (raw, SQL) show **Copy** (by selection id) + **Copy All**; non-TextEdit views keep one whole-content entry (`Label::selectable(true)` handles partial Ctrl+C).
- **Arrow navigation**: extend-selection (Ctrl+Arrow) resolved before plain arrows in `draw_table`. Jump-first/last are Ctrl+Shift+Arrow. `ScrollPageUp`/`ScrollPageDown` (Ctrl+PageUp/Down) step by `floor(area_height/row_height)` and let `scroll_row_into_view` follow. All remappable via `ShortcutAction`.
- **Zoom**: Ctrl+Plus/Minus/0, 5% steps, 25-500%; `OctaApp.zoom_percent` is transient.
- **Clipboard**: dual-path `arboard` + `TableViewState.clipboard` fallback; TSV cells, newline rows.
- **Lazy row loading**: Parquet loads `initial_load_rows` sync, then `bg_row_buffer`/`bg_loading_done`/`bg_can_load_more` background-stream (UI drains per frame).
- **Column types as strings**: `ColumnInfo.data_type` holds Arrow type names; all readers must round-trip through `data_type_from_string()`.
- **Unsaved-changes guards**: window close and file open check `is_modified() || raw_content_modified` -> Save/Don't Save/Cancel.
- **Read-only mode**: `OctaApp.readonly_mode` is a transient session flag (not persisted), F8 / **View -> Read-only mode**; every edit path funnels through `is_readonly()`. First toggle queues `ReadOnlyNotice` unless opted out. Status bar shows a plain `[Read-only]` pill.
- **JSON/YAML tree key editing**: `render_value_tree` overrides `selection.bg_fill` to a neutral; object keys renamable inline (Enter -> `json_util::rename_object_key_at_path`, rebuilds map to preserve order); array indices not. Edits flow to `tab.raw_content` via the kind's serializer.
- **Parse-error fallback**: `load_file` calls `fallback_to_raw_text` when a text-format reader errs — reloads via `TextReader`, switches to `ViewMode::Raw`, shows a dismissible `tab.parse_error_banner`. Files >500 MB skip it; binary formats never fall back.
- **Column-wide date promotion**: `run_date_inference_pass` promotes string columns all matching one date/datetime layout to `Date`/`DateTime`. Ambiguous columns queue `pending_date_pickers` (European/US/leave-as-text). Binary readers excluded.
- **Multi-file open**: `pick_files()` / CLI multi-path -> `pending_open_queue`; `drain_pending_open_queue` pops one per frame, pausing while a table/date-picker modal is up.
- **Raw CSV quote/escape modes**: `format_delimited_text` (`view_modes/raw_text.rs`) is a quote-aware tokenizer (RFC 4180 defaults `Double`+`Doubled`); `colored_layouter` reuses `split_delimited_line_ranges`. Coloring/alignment off via the slow-file prompt for CSV/TSV >10 MB.
- **Best-fit column width**: double-click a header seam -> `compute_optimal_col_width` (header + cells via `layout_no_wrap`, ≤5000 sample rows, +16px). Rust 2024 edition required for `cargo fmt`.
- **Parse in new tab**: scope (Cell/Row/Column/Whole table) + format (JSON/JSONL/YAML/TOML/XML/CSV/TSV/Markdown/Plain Text) -> a `tempfile::NamedTempFile`, routed through `load_file`; `source_path` cleared. Cell/Row/Column build a **synthetic `DataTable`** (source names as headers) then serialize via the registry, so headers survive (Plain Text is verbatim). Helpers in `src/app/dialogs/parse_in_new_tab.rs`.
- **Value Frequency**: `Ctrl+Shift+I` / header right-click / **Analyse -> Value frequency...**; no-context raises `value_frequency_picker.rs`. Compute `octa::data::value_frequency::compute_value_frequency`; UI `src/app/dialogs/value_frequency.rs`. Binning `Sturges` (`[5,30]`) or `Custom(n)` (`[1,1000]`); **binned results keep every bucket, ascending, not Top-N-truncated**; raw mode sorts by count desc with Top-N. "Filter table to this value" writes `column_filters`.
- **Numeric display formatting**: `src/data/num_format.rs` — `format_cell_number(value, Option<NumberFormat>, thousands, SeparatorStyle)` + `round_value`. `NumberFormat { decimals: Option<i32>, rounding }`; decimals signed (positive=after point, negative=round before, None=Auto). Per-column on `TabState.column_number_formats` (session-only). Configured via `src/app/dialogs/column_format.rs` (window id `octa_column_format_dialog_v2`; multi-column picker `column_format_cols`). Rounding is display-only: on Save if any column `rounds_values()`, `do_save_tab` defers to `pending_round_save` + `round_save_prompt.rs`.
- **Whitespace trim on load**: `octa::data::trim::trim_string_columns(&mut DataTable) -> Vec<String>` strips leading/trailing whitespace from string cells + column titles (interior kept). `run_trim_pass` runs in `apply_loaded_table` before date inference when enabled; for DB tables re-syncs `db_meta.original`/`original_columns`. With `warn_on_whitespace_trim`, sets `pending_trim_warning` -> dismissible banner in `central_panel.rs` (next to the date banner). Both load banners (date, trim) have **Okay** (accept + close; date keeps the promotion) and **Dismiss** (date reverts via `revert_promoted_date_columns`; trim just closes).
- **GUI glyphs are ASCII-only**: egui's bundled font renders `—`/`→`/`…`/`·` as tofu. Keep UI strings and prose ASCII (including `\u{2014}`-style escapes).
- **Excel multi-sheet**: `ExcelReader` implements `list_tables` + `read_table(sheet)` and overrides `opens_all_tables() -> true`. `load_file` branches: ≤ `excel_max_auto_sheets` opens each sheet in its own tab; above it raises `pending_sheet_picker` (`src/app/dialogs/sheet_picker.rs`, multi-select). Excel *write* is single-sheet.
- **Find duplicates**: **Search -> Find duplicates...** / **Ctrl+Shift+D**. `octa::data::duplicates::find_duplicate_rows` (text key via `CellValue::to_string()` joined `\x1F`). UI `src/app/dialogs/find_duplicates.rs`. `Highlight` marks `MarkColor::Orange`; `NewTab` clones. `int(1)` vs `float(1.0)` don't dedupe.
- **Archive viewer**: `.zip`/`.tar`/`.tgz` -> `octa::formats::archive_reader::ArchiveReader` (read-only); `read_file` returns one row per entry; `format_name` starts `"Archive"`. Action bar `src/app/archives.rs::render_archive_action_bar` when `active_tab_is_archive()`; **Open selected entry** -> `extract_entry_bytes` + tempfile + `load_file_in_new_tab` (pushes an empty placeholder so the listing isn't clobbered). `.tar.gz` not routed (ambiguous); `.tar.bz2` skipped.
- **Multi-search**: **Search -> Multi-search...** / **F6**. `octa::data::multi_search::search_table(...)` (reuses `RowMatcher`). `src/app/multi_search.rs` owns `MultiSearchState` (worker + cancel in `Arc`), bottom `egui::Panel`. Scopes `AllOpenTabs` (sync) / `Directory` (worker on `read_sorted_dir`, single level, per-file cap `grep_max_file_size_mb` default 50 MB). Skips push `SkippedFile {Oversized|ParseError}` -> "N skipped" chip. Caps 10k total / 1k per file.
- **Schema Export**: **File -> Export schema...** / **F7** (not Ctrl+Shift+X — collides with `Event::Cut`). Library `src/data/schema_export/{mod,sql,pydantic,typescript,json_schema,rust}.rs` — pure `(&[ColumnInfo], &str) -> String`. Nine targets: five SQL dialects (Postgres/MySQL/SQLite/Databricks/Snowflake) behind one `Dialect` enum; Pydantic/TypeScript/JSON Schema/Rust one file each. Surfaces: GUI `src/app/dialogs/schema_export.rs`, CLI `src/cli/export_schema.rs`, MCP `export_schema`. Idents via `sanitize_ident`+`is_safe_ident`; unknown Arrow types -> TEXT + comment. Adding a target: `pub fn export(...)` + a `SchemaTarget` variant + four match arms + `SchemaTargetArg`/`Target` mirrors.
- **Chart tab** (`ViewMode::Chart` + `TabState.is_chart_tab`): pure `build_chart(...) -> Result<ChartPrep, ChartError>` in `src/data/chart.rs`. Kinds Histogram (Sturges), Bar (`ChartLimits.max_categories` default 200), Line (X-sorted), Scatter, Box (Tukey, whiskers 1.5*IQR). Axis bounds on `ChartConfig` -> `Plot::default_*_bounds`. `sample_indices` honours `ChartLimits.max_points` (Bar/Box fold full input). `cell_to_f64` coerces Date->days, DateTime->epoch secs. A *tab not a view mode*: `open_chart_tab` clones into a fresh `TabState`; `available_view_modes` returns `[Chart]`. Trigger Analyse -> Chart / `OpenChart` (F5). UI `src/view_modes/chart.rs`; export `src/data/chart_export.rs` (SVG -> `resvg` PNG / `svg2pdf` PDF). Deps: egui_plot 0.35, svg2pdf 0.13.
- **Formula diagnostics**: `evaluate_formula_with_diagnostics` returns `FormulaOutcome { value, bad_cell }`; non-numeric/Date/DateTime/Binary/Bool/Nested -> `Err(FormulaBadCell)` (first bad cell short-circuits). `add_column.rs` shows a `Formula skipped N of M...` banner.
- **Custom title bar (opt-in)**: `use_custom_title_bar` strips system decorations; `draw_toolbar` renders close/max/min at the toolbar's right edge. Drag-to-move via WM convention (Alt/Super+drag). Takes effect after restart.
- **Cycle view mode (F4)**: advances `tab.view_mode` through `available_view_modes`; gated on TextEdit focus.
- **Malformed-file repair** (opt-in, default off): `maybe_offer_repair` runs `csv_reader::analyze_delimited`; if flagged raises `pending_file_repair` -> `src/app/dialogs/repair_file.rs`. `resolve_file_repair` reloads via `read_delimited_opts` (`ReadOptions { lossy_utf8, delimiter, strip_bom_controls }`). CSV/TSV only.
- **Date/Time calculation**: pure `octa::data::time_calc` — `evaluate_cell(op, &CellValue[, &CellValue])` for `TimeCalcOp::{Difference, AddSubtract, ConvertDuration, Extract}` (parse via `date_infer`, chrono arithmetic; month/year add clamps the day). **Edit -> Date/Time calculation...** -> `src/app/dialogs/time_calc.rs`, materialising a new column.

### Write Support

Most formats write. CSV preserves its `csv_delimiter`. Excel writes via `rust_xlsxwriter` (calamine read-only); only `.xlsx` round-trips. ODS has a hand-rolled OpenDocument 1.2 writer. TOML/YAML serialize through a JSON intermediary.

### Database write semantics (SQLite / DuckDB)

DB writes are **diff-based, never overwrite**. The reader snapshots row identity (`rowid` / `__octa_row_id`) + original cells into `db_meta`. On save, in one transaction: DELETE missing tags, INSERT `None`-tag rows, UPDATE rows whose cells differ. Schema changes rejected before touching the file.

### SQL view

Active table exposed as DuckDB temp table `data`. Each tab owns a long-lived `SqlWorkspace` (`TabState.sql_workspace: Option<SqlWorkspaceHandle>` — `Rc<RefCell<SqlWorkspace>>`), dropped on close. Workspace section lists tables + ATTACHed DBs with per-row [x]/[detach], [+ Add table...], [Attach database...]; [refresh] re-pushes the active table's edits (SQL sees a cached snapshot until then). Result actions [Run]/[Clear]/[Export...]/[Write result to DB...] (last -> `SqlWriteBackDialog` -> `SqlWorkspace::write_result_to_db`; writing into the open file's own `db_meta.table_name` with Append/Replace is rejected). Docks Bottom/Top/Left/Right via Settings; editor has a monospace line-number gutter sharing one `ScrollArea`. Template `SELECT * FROM data LIMIT {sql_default_row_limit}` via hint_text. Autocomplete chips at word-token end. Workspace state is session-only.

### Undo / Redo

`src/app/shortcuts_dispatch.rs` -> `OctaApp::do_undo`/`do_redo` (`src/app/edit_ops.rs`), which reset `filter_dirty`/`widths_initialized`. Gated on no TextEdit focus (so Ctrl+Z edits text there). `UndoAction` covers cell edits, structural mutations, colour marks; structural ops push to `undo_stack`, clear `redo_stack`.

### Colour Marking

`DataTable.marks: HashMap<MarkKey, MarkColor>` (`MarkKey::{Cell,Row,Column}`, precedence cell>row>column). **Edit -> Mark** / `Mark` (Ctrl+M) apply `default_mark_color` with precedence rows>cols>cells>cell. Context-menu uses the same precedence when the clicked target is in the selection, else colours the single target. `interaction.set_mark`/`clear_mark` carry `Vec<MarkKey>` for batch apply. Rainbow theme pins marked-cell text to `WHITE` (or `BLACK` via `MarkColor::needs_dark_text`).

### Auto-Update

Toolbar checker. `ureq` HTTP, `flate2`/`tar`/`zip` extraction. Logic in `src/main.rs` + `src/ui/toolbar.rs`.

### Testing

Integration tests in `tests/`. Binary fixtures (parquet, avro, arrow, xlsx) auto-generated by `tests/common/mod.rs::ensure_fixtures()`; text fixtures checked in. DB tests seed a `tempfile::NamedTempFile` via `rusqlite`/`duckdb`. SQL tests build `DataTable` literals and call `octa::sql::run_query`. `tests/sql_workspace_tests.rs` exercises multi-table JOINs, ATTACH (DuckDB + SQLite, with/without the bundled extension), name collisions, and write-back round-trips in every mode.

### Windows Build

`build.rs` uses `winresource` to embed manifest/icon/metadata. `windows/octa.exe.manifest` controls UAC + DPI.

### Containers

Headless `Dockerfile` (multi-stage `rust:1-bookworm` -> `gcr.io/distroless/cc-debian12`) for CLI + `--mcp`. GUI libs (GTK/X11) compile but aren't dlopen'd headless, so the runtime drops them. `docker run -v $PWD:/data octa --schema /data/f.parquet`; MCP via `-i ... --mcp`. Podman-compatible. Docs `docs/cli/docker.md`. Alpine/musl rejected (DuckDB/rusqlite bundle C++).

### i18n

Hand-rolled `src/i18n.rs` (spans the lib/bin split): `t(key)` + `set_language(code)`, TOML catalogs embedded from `locales/*.toml` (English master + 17 Latin-script langs: de/es/fr/it/nl/pt/pl/sv/da/no/fi/tr/id/vi/ro/hu/cs; Roboto covers the accents). `set_language` runs at `OctaApp::new` and on Settings apply (live). `every_language_covers_every_english_key` enforces key parity; `t()` falls back to English for any missing key. String migration is **incremental** — add a key to *every* locale then call `t()`. The five newest locales (id/vi/ro/hu/cs) translate the core UI; long hint paragraphs fall back to English. Cyrillic/CJK/Arabic deferred (need fonts + RTL).
