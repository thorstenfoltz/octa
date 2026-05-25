# Release notes — `feature/mcp`

A batch of new features, a major dependency upgrade, a published
documentation site, and a long list of UX polish that landed across
this branch.

## Major new features

- **Command-line interface (`octa --schema | --head | --convert | --sql`).**
  Flag-driven, mutually exclusive action flags (`disable_help_flag` so
  `-h` and `--help` both print the long form). Output is governed by
  `-f / --format {tsv|json|csv}` (TSV by default; JSON keeps native
  types). One file per action under `src/cli/`, so adding a new verb is
  a drop-in. Examples and CLI documentation ship in
  `docs/cli/octa.1.adoc` (rendered to a real `man` page) and
  `docs/cli/man-page.md`. The release pipeline installs `asciidoctor`,
  renders the man page, and bundles it into the Linux tarball;
  `install.sh` installs it to `$PREFIX/share/man/man1/octa.1`. Two
  later additions: `--export-schema` / `-e` reuses the GUI's
  schema-export targets (Postgres / MySQL / SQLite / Databricks /
  Snowflake DDL, Pydantic v2, TypeScript, JSON Schema, Rust), with
  the dialect selected via `-t / --target`; and a global
  `--rows N|all` flag overrides the streaming initial-load cap for a
  single invocation (installed as an `InitialLoadRowsGuard` so every
  action handler sees the lifted cap).
- **MCP server (`octa --mcp`).** Stdio JSON-RPC server built on `rmcp`
  + `tokio` (current-thread runtime), with eleven tools: `read_table`,
  `schema`, `list_tables`, `count_rows`, `run_sql`, `convert`,
  `export_schema`, `profile`, `find_duplicates`, `value_frequency`,
  `search`. One file per tool under `src/mcp/tools/`. Configurable
  defaults under **Settings → MCP** (row limit + cell byte cap; read
  once at server startup). Result envelopes carry `truncated` /
  `total_rows_available` / `cell_truncated` so clients can re-query.
  `octa --mcp` peels off before `cli::dispatch` so the GUI and other
  CLI paths never construct a tokio runtime. The cap model has two
  new escape hatches: `mcp_default_row_limit: Option<usize>` with
  `None` (or per-call `limit: 0`) meaning unlimited; and a per-call
  `unlimited: true` flag that also installs an `InitialLoadRowsGuard`
  inside the `spawn_blocking` closure, so the file-loader cap lifts
  for that single read. Combined they truly return every row on disk,
  not just every row in the streaming pre-load.
- **Compare view.** Two sub-modes selected by the Compare toolbar:
  **Text Diff** (`similar` crate, git-style side-by-side, 500 ms
  timeout) and **Row Hash Diff** (`blake3` over user-picked columns,
  Left-only / Right-only / Shared buckets, cross-format because the
  hash sees only `CellValue::to_string`). Three entry points: **View →
  Compare with…**, tab right-click → "Compare with active tab", and
  **F9** (`CompareSelectedTabs`) when exactly one tab is
  Ctrl-click-selected.
- **EPUB reader + reading view.** New EPUB format reader (`rbook`
  Apache-2.0; HTML → Markdown via `htmd`) produces a 3-column table
  (`chapter` / `paragraph` / `text`). The dedicated reading view
  shows the book title, previous / next buttons, a chapter combo, and
  a thumbnail strip of embedded images (decoded lazily). The flat
  table view is still available, so paragraph text can be searched and
  filtered.
- **GeoJSON Map view.** Slippy-map tiles via `walkers` (default OSM
  template, configurable under **Settings → Map → Tile URL template**).
  GeoJSON is registered as a first-class read-only format with a
  `__geometry: Utf8` column (WKT). Map view paints Points, Lines,
  Polygons (fill + outline), GeometryCollections (recurse). Toggle
  between **Tiles** and **Geometry only** for offline / blocked
  environments. Reset-view re-centres on the feature centroid.
- **Column Filter (Excel-style).** New dialog with column combo,
  scrollable value-checkbox list (5000-row cap with type-to-filter
  search), Select all / Select none for the visible subset, and Apply
  / Clear / Cancel. Three entry points: **Search → Column Filter…**,
  **Ctrl+Shift+F** (remappable), and right-click on a column header →
  **Filter values…** (pre-seeds the column). Multiple columns AND with
  each other and with the toolbar text search. Active filters surface
  via a small accent dot beside the column name and a clickable
  **Filter: N col** chip in the status bar.
- **Column Inspector.** Read-only modal listing every column with type
  + quick stats (numeric min/max, has-nulls, all-unique). Sort A → Z /
  Z → A is view-only and does not mutate the underlying column order.
  Row selection (click / Ctrl+click / Shift+click), Ctrl+C copies the
  selection as TSV, right-click context menu offers per-column copies
  ("Copy 'Type'", etc.). Double-click jumps to the column in the
  underlying table. Default shortcut **Ctrl+I**.
- **Parse cell / row / column / table in a new tab.** **Edit → "Parse
  in new tab"** submenu with Cell / Row / Column / Whole table
  scopes, mirrored on the cell right-click context menu. A modal picks
  the target format (JSON, JSON Lines, YAML, TOML, XML, CSV, TSV,
  Markdown, Plain Text) and — for CSV/TSV — a delimiter. Multi-cell
  scopes follow the JSON-reader convention: JSON wraps cell payloads
  as `[c1, c2, …]`; every other text format joins cells with
  newlines. Whole-table scope serialises through the chosen format's
  writer (same path as Save As). The result is routed through the
  standard reader pipeline via a `tempfile::NamedTempFile` named with
  the format's primary extension; the new tab's `source_path` is
  cleared so an accidental Save can't overwrite `/tmp`.
- **Published documentation site.** Full MkDocs site under `docs/`
  with installation, usage, view modes, CLI, MCP, settings,
  troubleshooting, reference. Deployed to GitHub Pages from CI. The
  in-app **F1 Documentation** is independently maintained with the
  same scope.
- **Chart tab (`Analyse → Chart`, default `F5`).** Plots the active
  table as a **Histogram**, **Bar**, **Line**, **Scatter**, or **Box**
  chart via `egui_plot`. Opens as its own tab (a clone of the source
  table) so several charts of the same data can run side-by-side. Bar
  charts always show category names on the X axis (never numeric
  indices), and Line / Scatter fall back to the same categorical
  treatment when X is non-numeric. Date / DateTime columns chart
  natively — internally coerced to days / seconds since the epoch,
  but tick labels render back as readable `YYYY-MM-DD` strings. A
  **Customise** collapsible exposes chart title, X / Y axis label
  overrides, legend on / off + corner position, grid toggle,
  per-series rename + custom-colour picker, hand-picked Y bounds
  (Min / Max / Step), integer-only ticks, and `log10` scale (axis
  formatter still shows original magnitudes via `10^n` notation).
  **Export** to PNG, SVG, or PDF — all three derive from one
  hand-emitted SVG so they look identical regardless of window size
  or DPI. Sampling kicks in above `chart_max_points` (default
  100,000) for Histogram / Line / Scatter; Bar honours
  `chart_max_categories` (default 200).
- **Multi-search panel (`Search → Multi-search…`, default `F6`).**
  Cross-tab + directory grep with a docked result list. Two scopes:
  **All Open Tabs** (synchronous walk of every loaded tab) and
  **Directory** (single-level walk on a background thread, hits
  stream into the panel as files finish parsing). Reuses the
  existing `RowMatcher` so Plain / Wildcard / Regex carry over. A
  per-file size cap (`grep_max_file_size_mb`, default 50 MB) skips
  giant files; oversized and unparseable files appear in an
  expandable "N file(s) skipped" warning chip that lists each
  file + reason without hiding actual hits. Result rows read
  `source · row · column — snippet`; clicking jumps to the cell
  (loading the file into a new tab if needed). Result cap 10k total /
  1k per file so a runaway regex can't pin the UI.
- **Value Frequency dialog (`Ctrl+Shift+I` or column-header
  right-click → "Value frequency…").** `value_counts()`-style top-N
  histogram for any column. Numeric columns can be Sturges-binned
  with non-finite floats (`NaN`, `±Inf`) surfaced as raw rows
  alongside the bins. Top-N presets (20 / 50 / 100 / 500 / All).
  Right-click a value → "Filter table to this value" writes into the
  existing per-column filter.
- **Find duplicates (`Search → Find duplicates…`, default
  `Ctrl+Shift+D`).** Picks dedupe-key columns then either highlights
  every duplicate row in orange marks or opens a new tab containing
  only the duplicates. Key columns seed from the current column /
  cell selection on dialog open. The companion **Edit → Mark →
  Clear all marks** entry wipes the highlights in one click without
  requiring a selection first.
- **Schema export (`File → Export schema…`, default `F7`).** Render
  the column list as **Postgres / MySQL / SQLite DDL**,
  **Pydantic v2** dataclass, **TypeScript** interface,
  **JSON Schema**, or a **Rust** struct. Identifier sanitisation per
  target: SQL idents are quoted (Postgres / SQLite `"…"`, MySQL
  `` `…` ``); Pydantic / Rust emit `Field(..., alias=…)` /
  `#[serde(rename = …)]` for invalid names. Adding a target is a
  drop-in `src/data/schema_export/<name>.rs`.
- **Archive viewer (`.zip` / `.tar` / `.tgz`).** Archives open as a
  read-only table listing each entry's `path`, `size_bytes`,
  `compressed_bytes`, `mtime`, `is_dir`, and `type`. An action bar
  above the table extracts the selected entry into a tempfile and
  routes it through `OctaApp::load_file`, so any reader Octa
  supports (CSV, JSON, Parquet, SQLite, …) works on entries inside
  an archive. `.tar.gz` is intentionally **not** auto-routed
  (single-extension matching can't disambiguate from `.csv.gz`);
  rename to `.tgz` or open through **File → Open → All files**.

## Smaller features and improvements

- **Reopen last closed tab (Ctrl+Shift+T).** Walks back through the
  last 10 closed tabs; works for both file-backed and scratch tabs.
- **Auto-fit all columns (Ctrl+Shift+W).** Same algorithm as
  double-clicking a header seam, applied across the whole table
  (sample-capped at 5000 rows).
- **Configurable initial-load row cap.** **Settings → Performance →
  Initial-load row cap** controls how many rows streaming readers
  (Parquet, CSV, TSV) pull into memory on first open. The setting is
  a process-wide `AtomicUsize`; changes take effect without restart.
- **Syntect syntax highlighting** in the raw text editor and Jupyter
  notebook code cells. Whitelist excludes formats with dedicated
  views (JSON / YAML / XML / Markdown / TOML / CSV / TSV). Size cap
  configurable under **Settings → Performance → Syntax-highlight size
  cap** (Bytes / KB / MB unit picker). Default cap 1024 KB.
- **SQL editor font picker.** **Settings → SQL → Editor font** —
  `JetBrainsMono` (bundled, OFL-1.1), `MatchUiFont`, or
  `SystemMonospace`.
- **SQL panel dockable to all four edges.** **Settings → SQL → Panel
  position**: Bottom (default), Top, Left, Right. The editor / result
  splitter inside the panel resizes independently.
- **User-extensible "open as text" extensions.** **Settings →
  Performance → Open as text** accepts a comma-separated list. The
  file picker's "All Supported" filter is unioned with the list, so
  custom log / config extensions become discoverable.
- **In-app window controls (close / minimize / maximize).**
  **Settings → Appearance → Window controls in toolbar** (default
  off). With it on, Octa requests an undecorated viewport and paints
  buttons at the right edge of the existing toolbar. The maximise
  button reads back from `ctx.input(|i| i.viewport().maximized)` so
  its state mirrors the live window. Takes effect after restart.
- **Status bar busy indicator.** Small spinner + short label
  ("Loading rows…" / "Updating…") whenever a background row-load is
  draining or the auto-updater is checking / installing. Idle frames
  stay silent.
- **Suppressed window-manager startup-notify cursor.** `octa.desktop`
  now sets `StartupNotify=false`, removing the bouncing clock cursor
  X11 / Wayland showed for ~5 s after launching from a desktop entry.
- **Save As respects active filters.** When a text search or column
  filter is active, Save As writes only the **currently visible**
  rows; the in-memory table and `source_path` are left intact (the
  status bar confirms the export). Regular Save (Ctrl+S) always
  writes the full table — filters never affect the source file.
- **Menu hover-switch restored.** After egui's 0.31 → 0.34 upgrade
  removed `MenuRoot::stationary_interaction`, top-level menus (File /
  Edit / View / Search / Help) needed a re-implementation. The new
  `top_menu_button` helper drives `Popup::from_response` with an
  explicit `SetOpenCommand` (Toggle on click, `Bool(true)` when
  hovering another button while a menu is open). Submenus inside each
  popup keep `SubMenuButton`'s built-in hover-open via the attached
  `MenuConfig`.
- **Default settings tuned for the typical-user experience.** Icon
  variant **Rose** (was `Random`), negative numbers in red **on**
  (was off), default mark color **Green** (was Yellow), notebook
  output **Beneath** (was Beside), syntax-highlight cap **1024 KB**
  (was 1 MB / 1,000,000 bytes), max recent files **10** (was 5).
- **Bundled Roboto Medium as the default proportional UI font.** The
  upstream `Ubuntu-Light` (egui's default) is by design a *light*
  weight; Roboto Medium reads heavier and more legibly across bars,
  tabs, column headers and the documentation, without going full bold.
- **Icon picker chip beside the variant label.** White / Black icon
  options stay readable in both themes — the swatch is a small
  rounded chip beside the label rather than the label's own colour.
- **Christmas overlay + welcome-screen snowfall.** Passive snowflakes
  in the corners on Dec 24-26; three clicks on the welcome-screen
  logo within 1.5 s triggers a 5-second deterministic snowfall. No
  interference with normal input.
- **Edit menu cleanup.** Shortcut suffixes were removed from menu
  entries that were getting visually noisy ("Undo" no longer reads
  "Undo (Ctrl+Z)"; bindings remain discoverable under **Settings →
  Shortcuts**).
- **Markdown view layout icons (👁 / 🔀 / 📝).** The Preview / Split /
  Edit toggle now uses supplementary-plane emoji that NotoEmoji
  ships, replacing earlier glyphs that rendered as tofu squares.
  Bumped to size 15 to match the body weight.
- **Selection stats footer.** Selecting more than one cell adds an
  accent-coloured pill to the status bar with the live rollup:
  numeric selections show **Count / Sum / Avg / Min / Max**, mixed or
  string selections show just **Count**. Selection sources fall
  through in the same priority order Ctrl+C uses (multi-cell →
  rows → columns); single-cell selections keep the existing
  Cell / Type info pill instead.
- **Hide / show columns.** Right-click any column header →
  **Hide column** to drop it from the view. Hidden columns are still
  written by Save / Save As; the renderer just zeroes their visible
  width. **Edit → Show hidden columns** (greyed when nothing is
  hidden) brings everything back. Per-tab and session-only — same
  precedent as the column filter set.
- **Copy column name(s).** Right-click a column header →
  **Copy column name(s)** copies header text to the clipboard.
  Multi-column when the right-clicked header is part of an existing
  column selection (joined with newlines); otherwise just the one.
- **Pin tab.** Right-click any file-backed tab → **Pin tab**.
  Pinned tabs render a `📌` prefix, hide their close button, and
  refuse to close on Ctrl+W or via the unsaved-changes prompt with a
  status-bar message. File paths are persisted under the new
  `pinned_tabs` field in `settings.toml`, so pinned tabs reopen on
  next launch (missing files are silently pruned). Scratch tabs (no
  source path) are excluded — the menu entry is greyed out for them.
  Pinning does **not** change save semantics: closing with unsaved
  changes still triggers the standard Save / Don't Save / Cancel
  prompt, and the reopened pinned tab reflects only what's on disk.
- **Analyse toolbar group.** Replaces the standalone SQL toolbar
  button with a single **Analyse** dropdown containing two
  independent entries: **SQL** (toggles the existing panel, same
  shortcut, no behaviour change) and **Chart** (opens a new chart
  tab). Both can be open / closed independently. The SQL panel
  itself gained a **×** close button in its header so dismissing it
  no longer requires reopening the toolbar dropdown.
- **British English across user-facing prose.** Documentation,
  README, in-app help, settings labels, and menu entries use British
  spelling throughout (`Analyse`, `Customise`, `Maximise`,
  `Minimise`, …). Field names + API mirrors stay American to match
  egui's underlying types.
- **Settings: numeric caps use thousand-separator text inputs.**
  **Multi-search file cap (MB)**, **Chart max points**, and
  **Chart max categories** swapped from `DragValue` (which shows the
  horizontal-resize cursor on hover) to plain `TextEdit`s with
  comma-separated input (`100,000`), matching the existing
  Initial-load row cap and SQL row limit pattern.
- **Date axis detection.** Histogram / Line / Scatter automatically
  detect when the X column is a `Date` or `DateTime` and format
  tick labels as `YYYY-MM-DD` (or `YYYY-MM-DD HH:MM:SS`) instead of
  raw days-since-epoch integers. Carries through to PNG / SVG / PDF
  exports.

## Format support

- **EPUB (`.epub`)** — read-only via `rbook` (Apache-2.0; the named
  upstream `epub` crate is GPL-3.0 and blocked by `deny.toml`). HTML
  → Markdown via `htmd`. Produces a 3-column paragraph table; the
  dedicated reading view is the default mode.
- **GeoJSON (`.geojson`)** — read-only via `geojson` + `wkt`. Always
  routed to `GeoJsonReader`; `JsonReader` was tightened to claim only
  `.json` so the registry's first-match-by-extension dispatch lands
  correctly.
- **ODS (`.ods`)** — dedicated read + write path. Reads via calamine,
  writes by hand-rolling an OpenDocument 1.2 package (`mimetype`
  stored uncompressed + `META-INF/manifest.xml` + `content.xml`).
- **PDF support removed.** The MuPDF dependency added a heavy system
  package requirement for a single read-only viewer; dropped along
  with its tests.

## Dependency upgrade (egui 0.31 → 0.34)

A single batch raised the entire egui ecosystem and aligned everything
around it:

- **MSRV → Rust 1.92** (egui 0.34 requirement).
- **egui / eframe / egui_extras 0.31 → 0.34.2.** Migrated `App::update`
  → `App::ui`, the unified `Panel` API, the new `Popup` machinery,
  fonts-mut, viewport command shapes (~21 call sites), `ui.close()`
  vs old `close_menu`, layouter closure signatures, text-edit
  response shape, and more.
- **walkers 0.42 → 0.53** — the Map view's plugin / projector API
  updated.
- **arrow / parquet 54 → 58.** ORC migrated off its `arrow58` alias
  onto the main `arrow`; an `arrow57` alias is kept for the SPSS
  reader (`ambers` still pins arrow ^57).
- **`egui_commonmark` dropped.** The in-app Documentation dialog and
  the markdown preview view both already had access to the custom
  `pulldown_cmark` renderer (`render_pulldown`); promoting it to
  `pub(crate)` let the docs dialog reuse it.
- **YAML: `serde_yaml` → `serde_yml` → `serde_yaml_ng`.** The first
  swap moved off the unmaintained upstream; the second was forced
  when a security advisory flagged `serde_yml` and its `libyml`
  dependency. `serde_yaml_ng` is the actively-maintained fork that
  Trivy and `cargo deny check licenses` both accept.
- **35+ other crate bumps** (rfd, arboard, calamine, rust_xlsxwriter,
  apache-avro, quick-xml, chrono, strum, clap, ureq, zip 2 → 8,
  pulldown-cmark, image, syntect, similar 2 → 3, blake3, regex,
  rand 0.10, hdf5-reader, dta 0.5 → 0.6, dbase, rds2rust, netcdf3,
  rmcp, tokio, duckdb, rusqlite, …).

## New chart-batch dependencies

- **`egui_plot 0.35`** for the on-screen chart rendering. The 0.34
  release of `egui_plot` still pulled `egui 0.33` (upstream bumped
  the inner dep without bumping the major) so we're on 0.35.0 — the
  first version that targets `egui 0.34`. MIT / Apache-2.0.
- **`svg2pdf 0.13`** for the PDF export path, with `default-features
  = false` plus `features = ["text"]` so we get `fontdb_mut()` on
  the usvg Options (otherwise PDF text elements drop and the
  rendered chart is mostly blank). Both `to_png` and `to_pdf` call
  `opt.fontdb_mut().load_system_fonts()` before parsing the SVG so
  axis labels, titles, and legend entries render correctly.
  MIT / Apache-2.0.

## CI, packaging, licensing

- **Dedicated `licenses` CI job** runs `cargo deny check licenses`
  against `deny.toml`. Copyleft licenses (AGPL/GPL/LGPL/SSPL) are
  excluded; the allowlist also covers MITNFA, MPL-2.0, and
  Unicode-DFS-2016 forward-compatibly.
- **`THIRD_PARTY_LICENSES.md` regenerated** from `Cargo.lock` via
  `cargo about generate about.hbs --output-file
  THIRD_PARTY_LICENSES.md`. `licenses/<SPDX-id>.txt` carries the
  canonical text for every license family used: MIT, Apache-2.0,
  BSD-2-Clause, BSD-3-Clause, BSL-1.0, CC0-1.0, CDLA-Permissive-2.0,
  ISC, OFL-1.1, Ubuntu-font-1.0, Unicode-3.0, Zlib.
- **Both `install.sh` and `install.bat`** now ship
  `THIRD_PARTY_LICENSES.md`, `LICENSE`, and the `licenses/` directory
  alongside the binary so installed copies meet Apache-2.0 / MIT /
  BSD / OFL attribution requirements.
- **`release.yml` does not re-run `cargo test`** — PR-time CI already
  validated the merged commit. The release jobs only build
  `cargo build --release` plus platform packaging.
- **Linux AppImage release artifact.** `release.yml::build-linux` now
  also builds an `Octa-${VERSION}-x86_64.AppImage` via `linuxdeploy`
  + `linuxdeploy-plugin-gtk` (rolling `continuous` releases) and
  uploads it alongside the tarball. Bundles GTK and every native
  runtime dep, so end users get a single self-contained executable on
  any reasonably recent Linux distro. FUSE-less hosts can fall back
  to `--appimage-extract-and-run`.
- **MegaLinter image is the stock `ghcr.io/oxsecurity/megalinter-rust:v9`**
  — Rust lints (fmt + clippy + tests) live in the `test` job to share
  the Swatinem Rust cache. Running them inside MegaLinter previously
  cost ~1000 s/PR because that container has no Rust cache.

## Fixes

- **Column Inspector: row selection, right-click menu, and Ctrl+C
  copy restored.** `egui_extras 0.34`'s `TableBuilder` defaults each
  cell's `Sense` to hover-only, which silently disabled every
  `response.clicked()` / `secondary_clicked()` check downstream.
  Adding `.sense(egui::Sense::click())` to the inspector's table
  builder restores click selection (with Ctrl / Shift modifiers),
  the per-row context menu, and the Ctrl+C → TSV path.
- **Column Filter: "Select none" actually selects nothing.** The
  "first-frame seed" logic was re-seeding the draft to "all values"
  every frame as long as the column had no saved filter — silently
  undoing every Select-none click. Replaced with an explicit
  one-shot `column_filter_needs_seed` flag set by
  `open_column_filter_dialog` / column-switch and consumed by the
  dialog after the initial seed.
- **Insert Column dialog: no more spinner-arrow artifacts on the
  position field.** Swapped `egui::DragValue` (which renders ±
  hover arrows that looked out of place in this small modal) for
  a plain `egui::TextEdit::singleline`. Out-of-range input tints the
  text red; the buffer is cleared on dialog close so the next open
  re-derives its default from the actual column count.
- **Insert Column dialog: formula no longer silently treats
  non-numeric cells as 0.** The formula evaluator previously
  coerced any non-numeric referenced cell to `0.0`, quietly
  corrupting results. It now reports the first offender via
  `FormulaOutcome { value, bad_cell }`. After adding the column the
  dialog raises a dismissible banner: `Formula skipped N of M
  row(s); first non-numeric reference: column "B" row 3 = "abc"`.
- **View → Read-only mode menu label no longer shows the shortcut.**
  The entry used to read `Read-only mode (F8)`; it now reads
  `Read-only mode`. F8 (or the user-rebound combo) still toggles it,
  and the read-only intro modal can be silenced via **Settings →
  File-Specific → Read-only mode notice**.
- **F1 documentation prose cleaned up.** Em-dashes removed in favour
  of commas / colons / parentheses so the in-app docs read more
  naturally and don't carry the AI-output tic.
- **Light theme readability bump.** `text_primary`, `text_secondary`,
  `text_muted`, and `text_header` darkened in the light theme so
  toolbar / tabs / column headers stay distinct against the now
  lighter bar surfaces.
- **Chart PNG / PDF export rendered mostly blank.** Root cause:
  `resvg::usvg::Options::default()` and
  `svg2pdf::usvg::Options::default()` both ship with an empty
  fontdb, so every `<text>` element (title, axis labels, legend,
  tick numbers) silently dropped. Both export entry points now call
  `opt.fontdb_mut().load_system_fonts()` before parsing the SVG;
  `svg2pdf` also has its `text` feature explicitly enabled so the
  accessor exists on its usvg side. SVG export already worked since
  the consumer (browser / Inkscape) loads its own fonts.
- **Histogram "Auto (Sturges)" checkbox could not be unticked.**
  The change-handler only acted when `auto == true`, so unticking
  set `hist_bins` to `None` then the next frame re-derived
  `auto = hist_bins.is_none() == true` — flipping it back on. Both
  directions now write back to the config so the checkbox actually
  toggles.
- **Bar chart X axis showed `0 / 1 / 2 / …` instead of category
  names.** `Plot::x_axis_formatter` now looks up
  `categories[mark.value.round() as usize]` so each integer tick
  carries its category label; the same formatter handles Box plots
  (Y column names) and categorical Line / Scatter (first-seen
  ordering). The numeric / Date / DateTime path remains unchanged.
- **Line / Scatter rejected non-numeric X with "not numeric".**
  Both kinds now probe the X column at build time and fall back to
  a categorical path when needed: each row sits at its
  first-seen-order category index, Line connects them
  left-to-right.
- **Hard-coded 200-category cap moved into Settings.** Bar /
  categorical Line / Scatter now honour
  `AppSettings.chart_max_categories` (default 200) instead of the
  fixed compile-time constant. Threaded through `ChartLimits` so
  the binary side can also pass a tighter cap if needed.
- **Customise panel layout flattened.** The original two-column
  grid stacked 12 rows of label + input pairs and ate most of the
  vertical space. Now laid out as three wrapping horizontal groups
  (labels / legend / grid on one row, the Y-axis controls on a
  second, the per-series rename + colour pickers on a third), so
  the chart itself gains ~200-300 px of vertical real estate. Drops
  to multiple lines naturally on narrow windows — no horizontal
  scrolling.
- **`DragValue` cursor flash on numeric inputs.** Replaced
  `DragValue` with `TextEdit::singleline` for the chart Customise
  inputs (Y Min / Max / Step, histogram bins) and the Performance
  settings (Multi-search file cap, Chart max points / categories)
  so hover no longer renders the horizontal-resize cursor.
  Mid-typing transients ("`1.2e`", "`-`") leave the underlying
  `Option<f64>` unchanged until the buffer parses.

## Documentation

- **GitHub Pages docs.** New site under `docs/` with Getting Started,
  Using Octa, Command Line, MCP Server, Reference, and Tips sections.
  Per-feature pages for every view mode (Compare, EPUB Reader, Map,
  Markdown, Notebook, JSON & YAML Tree, Raw Text, SQL), the Column
  Inspector, the Column Filter, search / filter, editing, formulas,
  marking, saving, tabs + sidebar, and the file-format catalog.
- **CLI man page** lives at `docs/cli/octa.1.adoc` and renders to a
  real troff page; mirrored at `docs/cli/man-page.md` for the website.
- **In-app F1 documentation** rewritten alongside the public site;
  every new feature carries an F1 section.
- **`CLAUDE.md` shrunk** from ~283 to ~150 lines, keeping only the
  non-obvious engineering invariants and dropping prose that
  duplicates what the code or `Cargo.toml` already says.
