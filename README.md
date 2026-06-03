# Octa

<p align="left">
  <img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

An application for viewing data files. Octa opens files in a spreadsheet-like table view with sorting, filtering, and search options. Writing is supported but limited. Octa is primarily a reader.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

## Preview

<!-- TODO: add screenshot -->

## Why Octa?

One native tool to open, inspect, query, and compare data files across 20+ formats (Parquet, CSV, JSON, Excel, SQLite, DuckDB, GeoPackage, Arrow, Avro, ORC, SAS, SPSS, Stata, RDS, HDF5, NetCDF, DBF, GeoJSON, EPUB, archives, and more)
without spinning up Python, opening a browser, or installing a heavyweight database client. Octa runs as a standalone binary on Linux, macOS, and Windows.

The same binary also speaks the Model Context Protocol over stdio (`octa --mcp`), so AI assistants and automation pipelines can read local files directly through Octa instead of round-tripping data through a custom script.

## Supported Formats

| Format                    | Read | Write | Notes                                                                                                  |
|---------------------------|------|-------|--------------------------------------------------------------------------------------------------------|
| Parquet                   | yes  | yes   | Lazy row loading for very large files; DuckDB-backed fallback reader.                                  |
| CSV/TSV                   | yes  | yes   | Auto-detected delimiter; quote-aware coloured raw view.                                                |
| JSON/JSON Lines           | yes  | yes   | Collapsible JSON Tree view with inline key / value editing.                                            |
| Excel                     | yes  | yes   | Opens every sheet as a tab (picker above a configurable cap). `.xlsx` round-trips; calamine read path. |
| ODS                       | yes  | yes   | Hand-rolled OpenDocument 1.2 writer.                                                                   |
| Arrow IPC / Feather       | yes  | yes   |                                                                                                        |
| Avro                      | yes  | yes   |                                                                                                        |
| ORC                       | yes  | yes   |                                                                                                        |
| HDF5                      | yes  | no    |                                                                                                        |
| NetCDF v3 (.nc)           | yes  | no    |                                                                                                        |
| SQLite                    | yes  | yes   | Multi-table picker; diff-based writes via rowid identity.                                              |
| DuckDB                    | yes  | yes   | Multi-table picker; SQL Query view exposes the file as `data`.                                         |
| GeoPackage (.gpkg)        | yes  | yes   | Multi-table picker.                                                                                    |
| SAS (.sas7bdat)           | yes  | no    |                                                                                                        |
| SPSS (.sav, .zsav)        | yes  | yes   |                                                                                                        |
| Stata (.dta)              | yes  | yes   |                                                                                                        |
| R (.rds, .rdata)          | yes  | no    | `data.frame` / `tibble` only.                                                                          |
| DBF/dBase (.dbf)          | yes  | yes   |                                                                                                        |
| XML                       | yes  | yes   |                                                                                                        |
| TOML                      | yes  | yes   |                                                                                                        |
| YAML                      | yes  | yes   | Collapsible YAML Tree view (mirrors JSON Tree).                                                        |
| Jupyter Notebook          | yes  | yes   | Notebook view renders code + markdown cells with syntect highlighting.                                 |
| Markdown                  | yes  | yes   | Rendered preview, Split, and Edit modes.                                                               |
| EPUB                      | yes  | no    | EPUB Reader view, chapter-by-chapter with embedded images.                                             |
| GeoJSON (.geojson)        | yes  | no    | Map view with OSM tile rendering or geometry-only fallback.                                            |
| Archive (zip / tar / tgz) | yes  | no    | Read-only listing; per-entry extract-and-open action.                                                  |
| Fixed-width (FWF)         | yes  | no    | `.fwf` / `.prn`; read-only, best-effort column-boundary inference.                                     |
| Source code / config      | yes  | yes   | `.py`, `.rs`, `.go`, `.ts`, ... opened with syntect highlighting in the Raw view.                      |
| Plain Text                | yes  | yes   | Syntect highlighting for languages without a dedicated view.                                           |

Unknown file extensions are opened as plain text.

## Features

### Table View

- Virtual table rendering with smooth scrolling for large datasets
- Lazy row loading for Parquet files (handles millions of rows; cap configurable in **Settings → Performance**)
- Inline cell editing with type-aware parsing
- Column resize, drag-and-drop reorder, and double-click best-fit width (**Ctrl+Shift+W** fits every column)
- Ascending/descending sort by any column
- Cell, row, and column selection with clipboard copy/paste
- Search and filter across all columns in real time (Plain / Wildcard / Regex modes)
- Excel-style formulas in cells (`=A1+B1`) and as the "Insert column" formula
- Thousand separators for numeric cells with English (`1,234.56`) / European (`1.234,56`) styles, plus per-column rounding (right-click a column → **Number format…**;
  negative decimals round before the point). Both are display-only and never change saved data

### Multiple View Modes

- **Table** — structured spreadsheet display (default)
- **Raw Text** — source text with line numbers and optional column alignment.
  Syntect-based syntax highlighting kicks in for languages with no dedicated
  view (Python, Rust, shell, Terraform, etc.); the size cap is configurable.
  Shows a dismissible banner when format parsing failed and the file fell back
  to plain text.
- **Markdown** — rendered CommonMark preview with Preview / Split / Edit toggle; Split places a TextEdit next to a live preview.
- **JSON Tree** / **YAML Tree** — collapsible Firefox-style tree for `.json` / `.jsonl` / `.yaml` / `.yml`. Keys are renamable, values are editable, and you can add keys to objects in place.
- **Notebook** — rendered Jupyter notebook with code cells, markdown cells, and outputs. Code cells use syntect highlighting.
- **EPUB Reader** — chapter-by-chapter rendered text for `.epub` files. Top toolbar shows the book title, Previous/Next, and a chapter combo. Embedded images render as a thumbnail strip below the chapter body.
- **Map** — slippy-map view for `.geojson` files. OSM tiles (configurable URL) with feature geometries painted on top. Toolbar toggles Tiles ↔ Geometry-only; plain mouse-wheel zoom; double-click to zoom in.
- **Compare** — side-by-side comparison of two files. Two sub-modes toggle in
  the Compare toolbar: **Text Diff** (git-style line diff) and **Row Hash Diff**
  (BLAKE3-hashed columns; uniques + shared rows bucketed). Cross-format works
  since hashing sees cell text only.
- **SQL Query** — write a query against the current table (exposed as `data`) and see results beneath. Line numbers, chip-style autocomplete, UPPER/lower case conversion.

Press F4 to cycle through the available view modes for the current tab. F8 toggles a session-only **read-only mode** that disables every editing path while still allowing copy and Save-As.

### Editing

- Insert, delete, and move rows and columns
- Colour marking for cells, rows, and columns with six colour choices
- Undo / Redo for cell edits, structural changes, and colour marks
- Leading/trailing whitespace trimmed from string cells and column titles on load (configurable, with a banner listing the affected columns)
- Unsaved-changes guards on close and file open
- Save in the original format or export to a different one via Save As
- **Reopen Last Closed Tab** (default **Ctrl+Shift+T**) restores accidentally-closed tabs
- **Find duplicates** (default **Ctrl+Shift+D**) picks dedupe-key columns and either highlights duplicate rows or opens them in a new tab

### Inspecting data

- **Column Inspector** (default **Ctrl+I**) — schema-level overview of every column with types, null counts, and basic stats
- **Value Frequency** (default **Ctrl+Shift+I**, or **Analyse → Value frequency…** with a column picker) — `value_counts()`-style top-N values for any column.
  Numeric columns can be turned into a histogram: type a bin count (or leave it for automatic Sturges binning) and get that many equal-width ranges with their counts
- **Schema Export** (default **F7**) — render the column list as Postgres / MySQL / SQLite / Databricks / Snowflake DDL, Pydantic v2, TypeScript interface, JSON Schema, or a Rust struct. Also available from the CLI (`octa --export-schema`) and over MCP.
- **Chart** (default **F5** / **Analyse → Chart**) — open a new tab plotting the active table as a histogram, bar, line, scatter, or box chart via `egui_plot`.
  Customisable title / axis / legend / per-series colour, PNG/SVG/PDF export, log scale. See [Chart](docs/usage/chart.md).

### Archives

`.zip`, `.tar`, and `.tgz` files open as a read-only table listing
each entry's `path`, `size_bytes`, `compressed_bytes`, `mtime`,
`is_dir`, and `type`. An action bar above the table extracts the
selected entry into a tempfile and opens it as a new tab through
the normal file-open path — so any reader Octa supports (CSV,
JSON, Parquet, …) works on entries inside an archive. `.tar.gz` is
not auto-routed; rename to `.tgz` or use **File → Open → All
files**.

### Command-line

Octa is also a CLI. With no flags it launches the GUI; with one of the action flags it runs that action and exits:

```bash
octa --schema data.parquet                  # print column schema
octa --head data.csv -n 5                    # first N rows (default 20)
octa --head data.csv -n 5 -f json            # output as JSON instead of TSV
octa --convert in.csv out.parquet            # convert formats
octa --sql data.parquet -q 'SELECT count(*) FROM data'
octa --export-schema data.parquet -t snowflake   # schema as DDL / model / struct
```

Output format is selectable with `-f / --format {tsv|json|csv}` (TSV default). Run `octa --help` for the full reference.

### MCP server

`octa --mcp` starts a Model Context Protocol server on stdio. A suite of tools
covers the CLI surface plus inspection and write helpers: `read_table`, `tail`,
`sample`, `schema`, `list_tables`, `count_rows`, `run_sql`, `convert`,
`export_schema`, `profile`, `find_duplicates`, `value_frequency`, `search`,
`compare_schemas`, `diff_tables`, `describe_file`, `validate_against_schema`,
`unique_columns`, `write_table`, and `edit_table`. Defaults (row limit + per-cell
byte cap) are configurable under **Settings → MCP**. Add it to any MCP client
(Claude Desktop, Claude Code, MCP Inspector) and the model can answer questions
about your local data files.

### Settings

- Configurable font size and theme (light / dark, default switchable)
- JetBrains Mono / system mono / match-UI font picker for the SQL editor
- Per-format performance knobs: streaming row cap, syntax-highlight size cap
- User-extensible "open as plain text" extension list
- Remappable keyboard shortcuts

### Other

- CSV delimiter auto-detection (comma, semicolon, pipe, tab) and manual selection
- Date inference for text-formatted columns (CSV, JSON, Excel, etc.) with an ambiguity picker for European vs US-format dates
- Auto-update check from GitHub releases
- Cross-platform: Linux, macOS, and Windows

## CLI & MCP Server

### CLI Usage

Octa ships a small CLI alongside the GUI. With no flags it launches the GUI; pass one of the action flags to run that action and exit:

```bash
octa --schema data.parquet      # print column schema
octa --head data.csv -n 5       # preview the first rows
octa --describe data.csv        # format + size + schema + sample in one call
octa --help                     # full reference with worked examples
```

Action flags are mutually exclusive. The full set is documented in [`docs/cli/`](https://thorstenfoltz.github.io/octa/cli/) and via `octa --help`. Output format for table-printing actions is selectable with `-f / --format {tsv|json|csv}` (TSV default).

These flags work identically across every distribution channel: a plain binary off the releases page, an `install.sh` install, the AUR package, or an AppImage.

```bash
./Octa-x86_64.AppImage --schema myfile.parquet
```

### MCP Server

`octa --mcp` starts a stdio-based [Model Context Protocol](https://modelcontextprotocol.io/) server that exposes Octa's file-reading capabilities as MCP tools.
AI assistants (Claude Desktop, Claude Code, MCP Inspector, any MCP-compatible client) and automation pipelines can then read local Parquet, CSV, DuckDB, SQLite, Excel,
and every other supported format directly, without scripting a Python or JS shim in between.

The startup banner reports the resolved defaults:

```text
$ octa --mcp
octa --mcp ready (default response row limit: 1000, cell cap: 65536 bytes, file-loader cap: 5000000; override per-call via `limit` / `unlimited`)
```

What those defaults mean:

- **Row limit (1000)** caps how many rows the *response* carries, so an AI client doesn't flood its context with millions of rows by accident.
- **Cell cap (65,536 bytes)** guards against a single oversized BLOB or long-text cell dominating the response. Oversized cells get a `[truncated: ...]` marker pointing the model at `run_sql` to slice the value.
- **File-loader cap (5,000,000 rows)** is the streaming-format safety net for very large files (Parquet, CSV, TSV).

Every limit is overridable per call: pass `limit: 0` for an unlimited response, and `unlimited: true` on any read-bearing tool to lift the file-loader cap for that single call.
Defaults themselves live under **Settings → MCP** and **Settings → Performance**.

`octa --mcp` works with every distribution format: plain binary, `install.sh`, AUR package, and AppImage. No wrapper script and no separate install step are needed; the same binary that opens the GUI is the MCP endpoint.

#### Claude Desktop config example

```json
{
  "mcpServers": {
    "octa": {
      "command": "/path/to/octa",
      "args": ["--mcp"]
    }
  }
}
```

`/path/to/octa` can be a system-installed binary (`/usr/local/bin/octa`), a user-local install (`~/.local/bin/octa`), or an AppImage path (`/home/you/Octa-x86_64.AppImage`).
See [`docs/mcp/setup.md`](https://thorstenfoltz.github.io/octa/mcp/setup/) for Claude Code, MCP Inspector, and other clients.

## Docker / Containers

A headless container image ships the CLI actions and the `--mcp` server (no
GUI, since the windowing libraries are never loaded on the headless paths). It
is published to the GitHub Container Registry, so no Rust toolchain or local
build is needed:

```bash
docker pull ghcr.io/thorstenfoltz/octa:latest
```

Mount a data directory and pass any CLI flag (the binary is the entrypoint):

```bash
docker run --rm -v "$PWD:/data" octa --schema /data/file.parquet
```

Run the MCP server over stdio with an interactive stdin (`-i`):

```bash
docker run --rm -i -v "$PWD:/data" octa --mcp
```

The same `Dockerfile` works with Podman (swap `docker` for `podman`). See
[`docs/cli/docker.md`](https://thorstenfoltz.github.io/octa/cli/docker/) for
building from source, writing output as a non-root user, and wiring the
container into Claude Code.

## Installation

### Linux

The simplest option is to **download a pre-built binary** from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and run it
directly — no installation step is required:

```bash
chmod +x octa
./octa                  # run from anywhere
# or place it on your PATH, e.g. ~/.local/bin/octa
```

An [AppImage](https://appimage.org/) is also published alongside each
release for users who prefer a single self-contained file:
`chmod +x Octa-*-x86_64.AppImage && ./Octa-*-x86_64.AppImage`. See
[Installation → AppImage](docs/getting-started/installation.md#appimage)
for the FUSE-less fallback.

Use `install.sh` only if you want Octa to appear in your application
launcher with an icon and file associations, or if you want to build from
source. The script detects whether a pre-built `octa` binary is next to it
and uses it; otherwise it builds from source, which requires the Rust
toolchain (install from <https://rustup.rs/>).

```bash
# System-wide (installs to /usr/local)
sudo ./install.sh

# User-local (no sudo needed)
./install.sh ~/.local
```

`install.sh` also installs a man page to
`$PREFIX/share/man/man1/octa.1`, so **`man octa`** works once the
install finishes. Release tarballs ship a pre-rendered `octa.1`;
source installs render it from `docs/cli/octa.1.adoc` on the fly if
`asciidoctor` is on `PATH`.

Building from source additionally requires a C compiler and the native
libraries listed in `CLAUDE.md` (GTK, fontconfig, freetype, etc.).
`asciidoctor` is optional but recommended (so the man page gets
installed).

### Arch Linux

Available on the AUR as `octa` (build from source) and `octa-bin` (prebuilt binary).
Both install `man octa` automatically.

```bash
paru -S octa
```

or

```bash
paru -S octa-bin
```

### Windows

The simplest option is to **download `octa.exe`** from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and run it
directly — no installation needed. Place it wherever you like (e.g. your
Desktop or `C:\Tools\`) and double-click to launch.

Optionally, `install.bat` copies the binary into `Program Files\Octa`,
generates an `.ico` (if ImageMagick is on PATH), and creates a Start Menu
shortcut. Right-click and choose **Run as administrator**. It does *not*
modify your `PATH`; open Octa via the Start Menu shortcut or by running
`"C:\Program Files\Octa\octa.exe"` directly.

**Windows SmartScreen warning:** Octa is not code-signed, so on first
launch Windows shows *"Windows protected your PC"*. Click **More info**,
then **Run anyway**. Subsequent launches open without the prompt.

### macOS

The simplest option is to **download the macOS `.app` bundle** from the
[releases page](https://github.com/thorstenfoltz/octa/releases). The
release artifact targets Apple Silicon. Drop `Octa.app` into
`/Applications` (or anywhere else) and double-click to launch.

**First-launch unsigned-app warning:** Octa is not code-signed or
notarized, so macOS quarantines the app the first time you launch it.
You'll see *"Octa.app cannot be opened because the developer cannot be
verified"*. Two ways around it:

- Right-click the app icon in Finder, choose **Open**, then click **Open**
  in the confirmation dialog. macOS remembers the choice for that copy
  of the app.
- Or remove the quarantine attribute from a terminal:

```bash
# Locate the bundle and confirm its quarantine attribute is present
find /Applications -maxdepth 1 -name "Octa.app" -exec xattr {} \;

# Strip the attribute (top-level only — macOS only quarantines the bundle)
xattr -d com.apple.quarantine /Applications/Octa.app

# If the strip above fails with "No such xattr: …" but the warning persists,
# fall back to the recursive form once (handles "Octa.app is damaged"):
# xattr -cr /Applications/Octa.app
```

To build from source, install the Rust toolchain (<https://rustup.rs/>)
and the native dependencies via Homebrew, then `cargo build --release`:

```bash
brew install harfbuzz freetype gtk+3
cargo build --release
```

The resulting binary lives at `target/release/octa`.

## Configuration

Settings are stored in:

- **Linux:** `$XDG_CONFIG_HOME/octa/settings.toml` (defaults to `~/.config/octa/settings.toml`)
- **macOS:** `~/Library/Application Support/Octa/settings.toml`
- **Windows:** `%APPDATA%\Octa\settings.toml`

## License

MIT
