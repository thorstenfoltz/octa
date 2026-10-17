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

`sudo ./install.sh` for system-wide on Linux, `./install.sh ~/.local` for user-local. `get-octa.sh` is the one-line `curl | bash` installer for released builds: Linux x86_64 (`/usr/local` with sudo, or a prefix argument) and macOS arm64 (`Octa.app` into `~/Applications`, `octa` linked into `~/.local/bin`, no sudo, no Gatekeeper prompt since curl sets no quarantine flag); it verifies the tarball against `SHA256SUMS`. On Windows: `install.bat` (system-wide, admin; bundled in the release zip) or `install.ps1` (per-user, no admin; run standalone from the repo root, NOT shipped in the zip since it downloads the zip itself) — the PowerShell script downloads the latest release, verifies the SHA256 against `SHA256SUMS`, installs into `%LOCALAPPDATA%\Programs\Octa`, and `Unblock-File`s the binary to dodge SmartScreen. Linux build needs `libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libfontconfig1-dev libfreetype6-dev`.

## Architecture

Native desktop GUI for tabular data, `eframe`/`egui` (immediate-mode), Rust edition 2024. Needs a C compiler (duckdb, rusqlite). The crate is both a library (`src/lib.rs` re-exports `data`, `formats`, `ui`) and a binary (`src/main.rs`); tests `use octa::data::*`.

**Renderer: `glow` (OpenGL), deliberately not eframe's default.** eframe's `default` feature selects **wgpu**, which links naga (a shader compiler), wgpu-core/-hal/-types, ash (Vulkan bindings), spirv and gpu-allocator: 23 crates and ~3.5 MB for a program that draws 2D tables and writes no shaders. `eframe` is therefore declared `default-features = false` with an explicit `["accesskit", "default_fonts", "glow", "wayland", "x11"]`. Do NOT "fix" it back to `default`. glutin covers every target: GLX+EGL on X11, EGL on Wayland, WGL on Windows, CGL on macOS (Apple's deprecated-but-present OpenGL; the fallback if that ever breaks is a per-target `[target.'cfg(target_os = "macos")']` block selecting wgpu there only).

**`winit` is a direct dependency on purpose.** eframe pins winit with `default-features = false`, and the only thing that re-enables `winit/default` is eframe's own `default` feature, which we turn off. Dropping it would silently lose `wayland-dlopen` (the binary would hard-link `libwayland-client.so` and refuse to start on X11-only machines) and `wayland-csd-adwaita` (Wayland window decorations, needed whenever `use_custom_title_bar` is off). winit is already in the tree, so the direct dep adds no crates.

### Module map

One line per directory; the per-file detail lives in the rule files below.

- `src/main.rs` - app entry + root state (`OctaApp`); top-level UI orchestration.
- `src/app/` - application state, tabs, file IO, dialogs, panels (SQL, chat, multi-search, clean-up).
- `src/data/` - pure engines: `DataTable` + one file per analysis feature. No egui.
- `src/formats/` - `FormatReader` trait + `FormatRegistry`, one file per reader.
- `src/sql/` - `SqlWorkspace` over an embedded DuckDB connection.
- `src/db/` - live database connectors, one file per engine.
- `src/cloud/` - S3 / Azure / GCS object storage (plus SFTP / WebDAV).
- `src/api/` - REST / JSON endpoints as a data source.
- `src/auth/` - browser OAuth sign-in shared by DB and cloud connectors.
- `src/git/` - thin wrappers over the `git` CLI (compare, history, sidebar marks).
- `src/mcp/` - the `--mcp` stdio server, one file per tool under `tools/`.
- `src/cli/` - one file per CLI action.
- `src/ui/` - egui widgets: table view, toolbar, settings, theme, shortcuts.
- `src/view_modes/` - one renderer per view mode.
- `src/diagnostics/` - debug log, panic hook, redacted report.
- `locales/` - 32 i18n catalogues; `src/i18n.rs` reads them.
- `docs/` - the mkdocs site; `tests/` - integration tests.
- `samples/` - one openable file per supported format, for eyeballing a reader
  change. Binaries are in Git LFS; `samples/README.md` says what each file is
  and how it was made. Adding a format means adding a sample.

## Conventions

These hold everywhere. Subsystem detail lives in `.claude/rules/`.

- **Documentation is dual.** Every user-facing feature is documented in BOTH the in-app Help (`src/app/dialogs/documentation/content/` + the `sections()` list in `documentation/mod.rs`) AND the mkdocs site (`docs/` + a `mkdocs.yml` nav entry). One without the other is an incomplete feature. The site's User guide is sorted by task, not by surface: a new page goes into its topic section of the nav and gets a row in that topic's `docs/guide/<topic>/index.md` table (app page / CLI flag / MCP tool); CLI and MCP pages link back to the app page under **See also** (`In the app:`).
- **Every new control gets a hover tooltip**, backed by an i18n `_hint` key present in all 32 locales. Attach it to the CONTROL, not only to its row label - a tooltip on the label does not answer a hover over the widget beside it. Disabled controls explain why they are disabled.
- **i18n is add-then-use**: add the key to *every* `locales/*.toml`, then call `t("key")`. `every_language_covers_every_english_key` enforces parity; `t()` falls back to English for a missing key. Locales are written natively and informally, never transliterated.
- **Rows of controls go through `octa::ui::control_row`** (`control_line` for a single-line row, the drop-in for `ui.horizontal` with the same return type; `control_row` for a wrapping row; `control_grid` for label/control pairs; `pin_control_height(ui)` first in the `show` closure of any other `egui::Grid` holding a ComboBox; `control_text_edit` for a text field). A bare `ui.horizontal` starts 18px tall while themed buttons are 26-30px, so a label or checkbox before the first button centres 4-5px high and a ComboBox after one sinks (egui's default style hides it, so eyeballing a toy repro proves nothing). Reported again and again as "fields not in line", title bars included; on 2026-10-06 every such row (~330) and every combo grid (32) was converted at once. Dialog min/max/close are `dialog_chrome::caption_button`s with painted icons, never text glyphs. Measured by `src/ui/control_row_tests.rs`; `tests/aligned_rows_tests.rs` fails on any new bare row or unpinned combo grid.
- **GUI glyphs are ASCII-only**: egui's bundled font renders `—`/`→`/`…`/`·` as tofu. Keep UI strings and prose ASCII (including `\u{2014}`-style escapes). This covers typographic punctuation only - never alphabets; Roboto covers Latin/Greek/Cyrillic and the bundled Noto subset covers CJK.
- **Menu ellipsis means "something opens"**: a menu entry ends in `...` **iff** clicking it opens a new tab or window (dialog, file picker, or a result tab like Summary/Chart). Entries that just execute in place get none - including panel toggles (SQL, Assistant, Multi-search) and in-place re-reads (View -> Reopen as). So does Transpose, although it opens a result tab (the user's call, 2026-10-06: it runs at once and asks nothing, so dots read as a broken promise). Enforced across all 32 locales by `i18n_tests::menu_ellipsis_means_something_opens`, which checks for the ellipsis *anywhere* in the string, not at the end: zh word order puts it mid-label. Add new menu entries to one of its two lists.
- **British English in prose** (colour, serialise). Prose only, never identifiers. No em-dashes or stylistic hyphens; use commas, periods, colons.
- **Never silence a clippy or compiler warning** to make CI green. No `#[allow]`, no `-A`, no relaxed `-W`. Fix the code.
- **Modular extensibility**: extensible sets are one file per instance (readers, CLI actions, MCP tools, DB engines, chat providers, view modes). Model new ones after `src/formats/mod.rs`.
- **Design for arbitrary N**, never special-case "two": join / compare / merge take list inputs.
- **No features that require the user to learn a syntax.**
- **Errors print as `{e:#}`, never `{e}`**: anyhow's plain Display shows only the outermost context, so a refused DB write reported "creating the target table" and dropped the server's reason (the same failure mode that once hid a quoting bug behind a bare "inserting rows"). The GUI's write-back dialogs already did this; `cli::dispatch` and `db_write_back`'s status line now do too.
- **Interaction structs**: UI components return plain structs; `main.rs` reads them and mutates state. No callbacks.
- **Edit overlay**: cell edits live in `DataTable.edits` until `apply_edits()`; structural mutations shift edit indices atomically.
- **Read-only mode**: `OctaApp::is_readonly()` is the single chokepoint every edit path funnels through. `OctaApp.readonly_mode` is a transient session flag (not persisted), F8 / **View -> Read-only mode**; first toggle queues `ReadOnlyNotice` unless opted out. Status bar shows a plain `[Read-only]` pill.

## Testing

Unit tests live in a sibling file, never inline: `foo.rs` declares `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` (`mod.rs` -> `mod_tests.rs`; a second test module `bar_tests` -> `foo_bar_tests.rs`). It is still a child module, so `use super::*` reaches private items. Integration tests in `tests/`. Binary fixtures (parquet, avro, arrow, xlsx) auto-generated by `tests/common/mod.rs::ensure_fixtures()`; text fixtures checked in. DB tests seed a `tempfile::NamedTempFile` via `rusqlite`/`duckdb`. SQL tests build `DataTable` literals and call `octa::sql::run_query`. `tests/sql_workspace_tests.rs` exercises multi-table JOINs, ATTACH (DuckDB + SQLite, with/without the bundled extension), name collisions, and write-back round-trips in every mode.

Almost every test calls the library directly; the **binary** surfaces get their own process-spawning suites (`CARGO_BIN_EXE_octa`, `OCTA_CONFIG_DIR` pointed at a tempdir so no run touches the developer's real `settings.toml`). `tests/cli_smoke_tests.rs` walks every file-based action through the real binary asserting exit code + first stdout line, plus the `--validate-schema` exit-1-on-drift contract, `-f tsv|json|csv`, `--rows`, and `detect_action`'s missing-companion errors. `tests/mcp_smoke_tests.rs` speaks newline-delimited JSON-RPC to `octa --mcp` over stdio (initialize -> `tools/list` -> `tools/call`), pins that every advertised tool has a description + object `inputSchema` (a `schemars` regression would otherwise only show up at an agent's first call), and re-checks the `--mcp-read-only` tool removal on the wire. Adding a CLI action or MCP tool means adding a case to the matching smoke test. `tests/{dedupe,anonymize}_cli_tests.rs` cover those two actions in depth. `tests/db_live_tests.rs` is env-gated (`OCTA_TEST_{POSTGRES,MYSQL,MSSQL,REDSHIFT,CLICKHOUSE,EXASOL,SNOWFLAKE,DATABRICKS,BIGQUERY,ORACLE,TRINO,ATHENA}_URL`) and **passes as a no-op** when a var is unset, so a green local run says nothing on its own. The `db-live` CI job supplies Postgres + MySQL and fails if either skipped (see `.claude/rules/packaging.md`); the other ten engines still only run against a server you point them at by hand.

## Detail lives in `.claude/rules/`

Each file below loads automatically when a file matching its `paths:` glob is
read. Read one directly when planning work that has not touched those files yet.

| Rule file | Covers | Loads for |
|---|---|---|
| `cli.md` | the flag-driven CLI, one file per action | `src/cli/**`, `src/main.rs`, `docs/cli/**` |
| `mcp.md` | the `--mcp` stdio server and its tools | `src/mcp/**`, `docs/mcp/**` |
| `database.md` | the twelve live DB engines, write-back, server copy | `src/db/**`, the SQLite/DuckDB readers |
| `chat.md` | the in-GUI assistant, providers, profiles, sandbox | `src/app/chat/**`, `src/app/chat_panel/**` |
| `sql.md` | `SqlWorkspace` and the SQL panel | `src/sql/**`, `src/app/sql_panel/**` |
| `formats.md` | readers, registry, loading passes, write options | `src/formats/**`, `src/app/file_io/**` |
| `data-engines.md` | the pure analysis engines and their four surfaces | `src/data/**` |
| `ui.md` | table view, chrome, view modes, dialogs, settings | `src/ui/**`, `src/view_modes/**`, `src/app/dialogs/**` |
| `cloud.md` | S3 / Azure / GCS objects as paths | `src/cloud/**` |
| `i18n.md` | the 32 catalogues and the tests that guard them | `locales/**`, `src/i18n.rs` |
| `packaging.md` | CI, release, Microsoft Store, licensing, updates | `.github/**`, `windows/**`, `Dockerfile` |
