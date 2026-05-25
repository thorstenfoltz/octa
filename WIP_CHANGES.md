# WIP changes — Octa feature batch v2

Compact note of what landed in the current `feature/mcp` branch so a future
session (or you in three months) can reconstruct the scope without trawling
`git log`. Updates here should be folded into CLAUDE.md and then deleted
when the batch is fully shipped.

## Landed

- **Phase A — Quick wins**
  - A1 SQL editor font: JetBrains Mono bundled in `assets/`, `AppSettings.sql_editor_font` enum (JetBrainsMono / MatchUiFont / SystemMonospace), Settings → SQL → Editor font combo. Ligature support documented as an egui limitation in the hover tooltip.
  - A2 Best-fit all columns (Ctrl+Shift+W, remappable). Public `TableViewState::fit_all_columns`. Dispatch sets `fit_all_columns_requested` flag; `draw_table` consumes it on the next frame (needs `&Ui` for font measurement).
  - A3 Easter eggs:
    - Christmas overlay (Dec 24-26 local date, passive) — six-armed snowflakes in corners via `Background` Area.
    - Snowfall (3 clicks within 1.5 s on the welcome-screen logo, 5 s burst, 70 deterministic particles). Welcome logo wrapped in `Sense::click()` + new `TableInteraction.welcome_logo_clicked` field.
  - A4 CI dedup: `cargo test` removed from `release.yml`'s three platform jobs. PR-time `ci.yml::test` already ran tests on the merged commit.
  - A6 Reopen last closed tab (Ctrl+Shift+T). `OctaApp.recently_closed_tabs: VecDeque<ClosedTabSnapshot>` capacity 10. Two snapshot kinds: Path (re-loads via `load_file`) and Scratch { raw_content, view_mode, format_name } for unsaved tabs.

- **Phase B — Formats & views**
  - B3 Syntect syntax highlighting in raw editor + notebook source cells. Whitelist in `src/ui/syntax.rs` excludes JSON/YAML/XML/Markdown/TOML/CSV/TSV (they have dedicated viewers). Configurable size guard `AppSettings.syntax_highlight_max_bytes` (default 1 MB) via Settings → Performance with a Bytes / KB / MB unit picker. Bundled `assets/Terraform.sublime-syntax` (MIT, hand-written) added to the syntax set via `SyntaxSet::into_builder()`.
  - B4 Compare view: `ViewMode::Compare`, two sub-modes (TextDiff via `similar`, RowHashDiff via `blake3`). Triggers: View → Compare with… (file picker), tab right-click → "Compare with active tab", or `CompareSelectedTabs` shortcut (default F9) when exactly one tab is Ctrl-click-selected. RowHashDiff displays *actual row content* (not just hex digests) under per-hash collapsibles; default column-render falls back to first 8 when nothing picked. `OctaApp.tab_multi_selection: HashSet<usize>` tracks Ctrl-click staging.
  - Edit menu shortcut suffixes removed ("Undo" not "Undo (Ctrl+Z)" etc.) — bindings discoverable via Settings → Shortcuts. `shortcuts: &Shortcuts` toolbar arg now `_shortcuts`.

- **Phase A4-adjacent — Configurable streaming reader cap**
  - `AppSettings.initial_load_rows` (default 1,000,000) exposed as process-wide `AtomicUsize` (`src/formats/mod.rs::{initial_load_rows, set_initial_load_rows}`). Applied at startup *and* from Settings apply path so changes take effect without restart. Settings inputs use comma separators via `status_bar::format_number` + new `parse_comma_number` helper.

- **Adjustment — User-extensible text extensions**
  - `AppSettings.text_mode_extensions: Vec<String>` lets users force unknown extensions into the `TextReader` path. Checked before the format registry lookup in `OctaApp::load_file`. File picker filter expands accordingly.
  - `TextReader.extensions()` claims `tf`/`tfvars`/`hcl` so Terraform files open natively.

## Newly landed (since last edit)

- **C1 CLI (flag-driven)** — `octa --schema`, `octa --head`, `octa --convert`, `octa --sql` (mutually-exclusive action flags, not subcommands), plus global `-f / --format {tsv|json|csv}`, `-n / --lines` for head, `-q / --query` for sql. Argument parsing via `clap = "4"` (derive); `-h` and `--help` show the same long-form output via `disable_help_flag` + `ArgAction::HelpLong`. `after_help` carries worked examples for every action including `--sql`. One file per action under `src/cli/`. `main.rs` runs CLI before eframe; no action flag falls through to the GUI with any positional files. Smoke-tested end-to-end on `tests/fixtures/sample.csv`.

- **B2 GeoJSON map view** — new `src/formats/geojson_reader.rs` using `geojson = "1"` (MIT/Apache-2.0). Read produces one row per `Feature` with a leading `__geometry: Utf8` column carrying WKT (via `wkt = "0.14"`) plus the union of every feature's `properties` keys in first-seen order. `read_with_features(path)` returns `(DataTable, GeoJsonExtras { features: Vec<MapFeature> })` so the Map view gets `geo-types::Geometry<f64>` without re-parsing. Registered between `epub_reader` and `sqlite_reader`; `JsonReader.extensions()` was tightened to claim only `.json` (it used to also claim `.geojson`, which now belongs to `GeoJsonReader` so the registry's first-match-wins dispatch routes correctly).
  - **`ViewMode::Map`** + new TabState fields: `geojson_features: Vec<MapFeature>`, `map_mode: MapMode`, `map_tiles: Option<Box<walkers::HttpTiles>>` (lazy — needs egui `Context`), `map_memory: Option<Box<walkers::MapMemory>>` (lazy). `available_view_modes` adds Map for tabs with `format_name == "GeoJSON"`; toolbar grows `has_map: bool` and a "Map View" radio.
  - **View** (`src/view_modes/map.rs`): top toolbar with feature count + Tiles/Geometry radio + Reset-view button. Tiles backed by `walkers = "0.42"` (MIT) — pinned to 0.42 because that's the highest version still on egui 0.31; walkers 0.43+ requires egui 0.32+, walkers 0.53 wants egui 0.34 which would be a multi-hour Octa-wide rewrite (Panel API merge, App::ui rename, etc.). Walkers spawns its own dedicated thread + private tokio runtime for tile fetches (see `walkers-0.42.0/src/io.rs::Runtime`), so the eframe app stays sync. Custom `TemplatedTileSource` lets the user repoint at any `{z}/{x}/{y}` URL via `AppSettings.map_tile_url_template` (default OSM). Geometry overlay implemented as a `walkers::Plugin` so it can use the `Projector` for lon/lat→pixel conversion. Paint primitives: Point/MultiPoint → 5 px circle; LineString/MultiLineString → connected line segments; Polygon/MultiPolygon → `convex_polygon` fill + outline stroke; GeometryCollection → recurse; Rect/Triangle handled too. Polygon holes are stroked but **not** cut out of the fill (egui has no even-odd rule), which is the v1 trade-off. OSM attribution rendered bottom-right when in Tiles mode.
  - **Settings → Map**: default mode (Tiles / Geometry only), fallback-to-geometry checkbox, tile URL template text input. `AppSettings.map_default_mode: MapMode`, `map_fallback_to_geometry: bool`, `map_tile_url_template: String`.
  - Smoke-tested via CLI: `octa --schema sample.geojson` shows `__geometry / name / population`; `--head` returns the three sample features with correct WKT; `--sql -q 'SELECT name, __geometry FROM data ORDER BY name'` round-trips through DuckDB. GUI rendering not yet end-to-end tested in this session.

- **B1 EPUB reader + reading view** — new `src/formats/epub_reader.rs` using `rbook = "0.7"` (Apache-2.0). The plan's originally-named `epub` crate is GPL-3.0 and blocked by `deny.toml`; `rbook` was the permissive substitute. HTML→Markdown conversion via `htmd = "0.5"` (Apache-2.0; same swap from the GPL `html2md`). `EpubReader::read_file` builds a 3-column `DataTable` (`chapter:Int64`, `paragraph:Int64`, `text:Utf8`) with one row per non-empty paragraph block in each chapter's converted Markdown. A separate `read_with_extras(path)` returns `(DataTable, EpubExtras { chapters_md, chapter_titles, image_bytes, title })` so the reading view gets the rich side-state without re-parsing the EPUB; this mirrors how YAML files have both a table and a tree value on the tab.
  - **`ViewMode::EpubReader`** + new TabState fields: `epub_chapters_md`, `epub_chapter_titles`, `epub_image_bytes`, `epub_image_textures` (lazy texture cache), `epub_active_chapter`, `epub_title`. Populated in `apply_loaded_table` when `format_name == "EPUB"`.
  - **View** (`src/view_modes/epub_reader.rs`): top toolbar with book title, Previous/Next buttons, and a chapter combo showing `N. <label> (i/total)`. Chapter body rendered via the existing pulldown-cmark pipeline — `markdown::render_pulldown` was promoted from `fn` to `pub(crate) fn` and the `markdown` submodule from `mod` to `pub(crate) mod` so the EPUB view can reuse 150 lines of event-walking without duplication. Reading width capped at `clamp(200.0, 900.0)` matching the Markdown view.
  - **Image handling (v1)**: pulldown-cmark walked once per chapter to extract image hrefs; matched against `epub_image_bytes` by exact-href / `/`-prefixed / basename fallback. Resolved images decode via `image = "0.25"` (`default-features = false`, only `png/jpeg/gif/webp`) into egui textures stored in `epub_image_textures`. Rendered as a thumbnail strip below the chapter content (capped at 200 px on the long axis). Inline-in-paragraph image positioning is deliberately deferred — the pulldown-cmark renderer is non-trivial to fork.
  - Routing: `available_view_modes` adds `ViewMode::Table` (paragraph table) and `ViewMode::EpubReader` for EPUB tabs; reader is the default. Toolbar grows a `has_epub: bool` and a "EPUB Reader" radio under the View menu.
  - Smoke-tested via CLI: `octa --schema lewis-lion.epub` → 3-col schema; `octa --head -n 5` → cover chapter + book title rows. Cover is detected by rbook's spine walk and surfaces as chapter 1.

- **C2 MCP server (`octa --mcp`)** — stdio JSON-RPC server built on `rmcp = "1"` + `tokio` (current-thread runtime). New `src/mcp/` module with `mod.rs` (`OctaMcpServer` struct, `#[tool_router]`/`#[tool_handler]` dispatch) and one file per tool under `src/mcp/tools/` for drop-in extensibility. Six initial tools: `read_table`, `schema`, `list_tables`, `count_rows`, `run_sql`, `convert`. Per-tool `Params` structs derive `serde::Deserialize` + `schemars::JsonSchema` (the macro derives the input JSON Schema). Tool descriptions are inlined as string literals at the `#[tool(...)]` site because rmcp's macro doesn't accept a `const &str` there — kept in sync with per-tool docstrings.
  - **Row + cell caps**: server reads `AppSettings.mcp_default_row_limit: Option<usize>` (default `Some(1000)`) and `AppSettings.mcp_default_cell_bytes: usize` (default 65,536 = 64 KiB). Result-bearing tools accept a per-call `limit` (with `0 = unlimited`) and surface `truncated` / `total_rows_available` / `cell_truncated` flags. Cell truncation replaces the value with a `[truncated: N bytes; cap M bytes. Slice the value with --sql / run_sql to fetch the rest.]` marker.
  - **Settings UI**: new collapsible **Settings → MCP** section with a row-limit text input + "Unlimited" checkbox and a cell-byte-cap input. Buffers seed from current settings on dialog open + reset; values applied on Apply. Notes that changes require an `octa --mcp` restart.
  - **Dispatch**: `cli::Action::Mcp` is recognised by `Cli::detect_action`, but `main.rs` peels it off before `cli::dispatch` so the GUI / other CLI paths never build a tokio runtime. `run_mcp()` builds a `current_thread` runtime, installs `tracing_subscriber` to stderr (JSON-RPC owns stdout), loads `AppSettings`, calls `mcp::run`. Logs to stderr include the resolved row + cell caps at startup so a user running `octa --mcp` in a terminal sees what's in effect.
  - **Blocking work** runs on `tokio::task::spawn_blocking` to keep rmcp's runtime responsive — `FormatRegistry` reads and DuckDB queries are sync.
  - Smoke-tested end-to-end: `initialize` → `tools/list` returns all six tools with correct JSON schemas; `tools/call` for `schema`, `read_table` (with `limit:2`), and `run_sql` (`SELECT count(*) FROM data`) all return well-formed responses against `tests/fixtures/sample.csv`.
  - Settings defaults are duplicated literally in `src/ui/settings.rs::default_mcp_row_limit` / `default_mcp_cell_bytes` (the lib can't reference `src/mcp/`; the binary owns mcp). Both spots cite the canonical 1000 / 64 KiB values.

## Not yet landed (pending tasks)

- A5 Documentation final pass (this WIP file is a partial substitute).
- D1–D5 Network DBs (Postgres / MySQL / MariaDB, profiles + keyring, schema browser sidebar, cross-DB SQL via DuckDB ATTACH, write-back).

## Files added/changed (notable)

**New:**

- `assets/JetBrainsMono-Regular.ttf` (OFL-1.1)
- `assets/Terraform.sublime-syntax` (MIT, hand-written)
- `src/ui/syntax.rs`
- `src/view_modes/compare/{mod,text_diff,row_diff,hash}.rs`
- `src/mcp/{mod,tools/{mod,read_table,schema,list_tables,count_rows,run_sql,convert}}.rs`
- `src/formats/epub_reader.rs`
- `src/view_modes/epub_reader.rs`
- `src/formats/geojson_reader.rs`
- `src/view_modes/map.rs`
- `WIP_CHANGES.md` (this file)

**Modified (high-leverage):**

- `Cargo.toml` (+ syntect, similar, blake3)
- `src/data/mod.rs` (ViewMode::Compare, CompareMode enum)
- `src/ui/settings.rs` (SqlEditorFont, SyntaxSizeUnit, parse_comma_number, settings fields, Performance section)
- `src/formats/mod.rs` (atomic INITIAL_LOAD_ROWS + set_/get_)
- `src/formats/text_reader.rs` (tf/tfvars/hcl)
- `src/app/state.rs` (TabState compare_*, OctaApp recently_closed_tabs, tab_multi_selection, welcome_logo_*, snowfall_until, ClosedTabSnapshot enum)
- `src/app/tabs.rs` (close_tab snapshot, reopen helper, Ctrl-click multi-select, right-click compare menu)
- `src/app/easter_eggs.rs` (christmas overlay, snow renderer)
- `src/app/file_io.rs` (text-mode override, begin_compare_with, begin_compare_with_tab)
- `src/app/shortcuts_dispatch.rs` (FitAllColumns, ReopenLastClosedTab, CompareSelectedTabs)
- `src/ui/shortcuts.rs` (3 new ShortcutAction variants)
- `src/ui/toolbar.rs` (Edit menu cleanup, View → Compare with… entry)
- `src/view_modes/raw_text.rs` (RawViewOpts struct, syntect layouter)
- `src/view_modes/sql.rs` (sql_font_family helper)
- `src/view_modes/notebook.rs` (syntect for code cells)
- `src/ui/theme.rs` (JetBrains Mono FontFamily registration)
- `.github/workflows/release.yml` (test dedup)
- `THIRD_PARTY_LICENSES.md` (regenerated for new deps)

## Doc TODOs for the final pass

- Update CLAUDE.md "Module Layout" block to add `src/view_modes/compare/`, `src/ui/syntax.rs`, easter-egg snow/christmas.
- Add a "Compare view" section under "Key Design Patterns" describing the three trigger paths + bucket structure.
- Update the in-app docs dialog (`src/app/dialogs/documentation.rs`) with View → Compare with…, the Performance settings section, the syntect-whitelist behaviour.
- Mention the F9 default for `CompareSelectedTabs` in the shortcut list.
- Remove WIP_CHANGES.md and squash its content into CLAUDE.md once the rest of the batch lands.
