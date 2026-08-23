---
paths:
  - "src/cli/**"
  - "src/main.rs"
  - "docs/cli/**"
---
# CLI

`octa` is **flag-driven**, not subcommand-driven: pick one of `--schema`, `--head`, `--tail`, `--sample`, `--convert`, `--sql`, `--export-schema` (`-e`), `--compare-schemas`, `--diff`, `--describe`, `--validate-schema`, `--unique-columns`, `--anonymize`, `--dedupe`, `--impute`, `--outliers`, `--detect-pii`, `--union`, `--join`, `--partition-by`, `--resample`, `--rolling`, `--batch-convert`, `--drift-report`, `--check`, `--relationships`, `--mcp` (mutually exclusive via clap `group = "action"`); with none, launches the GUI with the positional file list. Implemented in `src/cli/`, one file per action. Global `-f / --format {tsv|json|csv}` (default tsv) routes through `src/cli/output.rs`; `--convert` (whose OUT may be `-` for stdout, requiring `--to EXT` since a pipe has no name; the data goes to stdout and the counts to stderr), `--export-schema` and `--partition-by` ignore it (the last prints a bare `path\trows` listing of the files it wrote, headerless, straight from `println!` - deliberately not a data table). `--export-schema` picks a target via `-t / --target` (`SchemaTargetArg` → `octa::data::schema_export::SchemaTarget`). `disable_help_flag = true` + a custom `--help` (`ArgAction::HelpLong`) makes `-h`/`--help` identical.

`--validate-schema` returns exit `1` on a successful read where schemas drift (CI gating); `cli::dispatch` pulls it out of the normal `Result → ExitCode` mapping so `validate_schema::run` returns `Result<ExitCode, _>`. `--mcp` does **not** route through `cli::dispatch`: `main.rs::run_mcp` peels it off first so GUI/CLI paths never build a tokio runtime.

`--diff` takes a `--diff-mode {set|ordered|join}` (default `set`) + `--diff-on COLS` (comma list, required for `join`). `set` is the original whole-row membership diff (`octa::data::diff::diff_rows`); `ordered`/`join` route through `octa::data::compare` (`compare_ordered` positional row-by-row cell diff; `compare_join` key-matched added/removed/changed). `compare::build_compare_table` renders the shared output table (leading `status` + `changed_columns`). `compare::subset` is reused by the MCP tool.

`--describe` grew `--deep` (physical layout, see File internals below); `--diff` grew `--diff-db CONN --diff-db-table SCHEMA.TABLE`, which replaces the second positional file with a live table (`--diff` is `num_args = 1..=2` for that reason, `detect_action` enforces which count each form needs); `--convert` and `--batch-convert` grew `--compression CODEC` (validated at parse time against `write_options::PARQUET_CODECS`) and `--row-group-size N`.

Man page source `docs/cli/octa.1.adoc`; the release renders/bundles `octa.1` (install.sh falls back to rendering); mkdocs mirrors it at `docs/cli/man-page.md`.

A positional FILE may be **`-` for stdin** (buffered to a temp file since every reader needs `Seek`, then `sniff_format`; empty stdin is an error), a cloud object URL (see Cloud URL as a path below) or a plain `http(s)://` URL (`cloud::fetch::{is_http_url,fetch_http_to_temp}`, no credentials, extension taken from the URL path ignoring the query string, non-2xx is an error naming the status so a 404 HTML page is never parsed as a table); both read-only from the CLI, and both are branched in `cli::read_table` and `ToolContext::resolve`. **`fetch_http_to_temp` takes a `UrlTrust`**: `UserSupplied` (CLI/GUI, unrestricted - reading `localhost:8000` is normal) vs `AgentSupplied` (MCP/chat), which resolves the host and refuses non-global addresses (loopback/private/link-local incl. 169.254.169.254/unique-local v6) and disables redirects, because the HTTP branch sits *before* the chat sandbox's `ensure_readable` and would otherwise be a way out of a `restrict_filesystem` profile. Ceiling: resolve-then-connect, so DNS rebinding is not covered.

Global `--rows N|all` overrides the streaming initial-load cap for one run (`cli::parse_rows_flag`; `all` → `usize::MAX`); `cli::dispatch` installs an `octa::formats::InitialLoadRowsGuard`.

`--sql` workspace flags: `--sql-table NAME=PATH` (repeatable) adds workspace tables; `--sql-attach ALIAS=PATH` ATTACHes a DuckDB/SQLite file (`alias.schema.tbl`); `--sql-write-to PATH` + `--sql-write-table TABLE` (+ optional `--sql-write-schema`, `--sql-write-mode {create|append|replace}`) persists the SELECT instead of printing. `src/cli/sql.rs` builds + tears down a fresh `SqlWorkspace` per invocation.

Data-ops actions (engines in `src/data/<name>.rs`, all also GUI dialogs + MCP tools): `--dedupe` (+`--dedupe-on/-keep`), `--impute COL=STRATEGY` (mean/median/mode/ffill/bfill/`const:VALUE`), `--outliers` (+`--outlier-method/-cols/-k`, prints flagged cell coords), `--detect-pii` (+`--pii-sample`), `--union` (+`--union-file/-drop/-cast`), `--join` (+`--join-file/-on/-type`), `--partition-by COL` (+`--out-dir` required, `--partition-format`, `--partition-layout {flat|hive}` where hive writes `<col>=<value>/data.<ext>` that dataset mode reads back with the partition column restored, writes N files). `tests/dedupe_cli_tests.rs` covers `--dedupe`.

Adding an action: append a flag to `cli::Cli`, add an `Action` variant, drop `src/cli/<verb>.rs`, extend `Cli::detect_action`, add a `cli::dispatch` arm.
