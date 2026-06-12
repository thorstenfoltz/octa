# Octa

<p align="left">
<img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

An application for viewing data files. Octa opens files in a spreadsheet-like table view with sorting, filtering, and search options. Including CLI, MCP and a chat assistant within the GUI.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

## Contents

- [Why Octa?](#why-octa)
- [Supported Formats](#supported-formats)
- [Features](#features)
  - [Table View](#table-view)
  - [Multiple View Modes](#multiple-view-modes)
  - [Editing](#editing)
  - [Inspecting data](#inspecting-data)
  - [Archives](#archives)
  - [Command-line](#command-line)
  - [MCP server](#mcp-server)
  - [Assistant (in-app chat)](#assistant-in-app-chat)
  - [Settings](#settings)
  - [Other](#other)
- [Docker / Containers](#docker--containers)
- [Installation](#installation)
  - [Linux (and WSL)](#linux-and-wsl)
  - [Linux AppImage](#linux-appimage)
  - [Linux (build from source)](#linux-build-from-source)
  - [Arch Linux](#arch-linux)
  - [Windows](#windows)
  - [macOS](#macos)
- [Configuration](#configuration)
- [License](#license)

## Preview

![Open a file](docs/assets/screenshots/first-steps-file-menu.png)

## Why Octa?

One native tool to open, inspect, query, and compare data files across 20+ formats (Parquet, CSV, JSON, Excel, SQLite, DuckDB, GeoPackage, Arrow, Avro, ORC, SAS, SPSS, Stata, RDS, HDF5, NetCDF, DBF, GeoJSON, EPUB, archives, and more)
without spinning up Python, opening a browser, or installing a heavyweight database client. Octa runs as a standalone binary on Linux, macOS, and Windows.

The same binary also speaks the Model Context Protocol over stdio (`octa --mcp`), so AI assistants and automation pipelines can read local files directly through Octa instead of round-tripping data through a custom script.

## Supported Formats

| Format                    | Read | Write | Notes                                                                             |
|---------------------------|------|-------|-----------------------------------------------------------------------------------|
| Parquet                   | yes  | yes   | Lazy row loading for very large files                                             |
| CSV/TSV                   | yes  | yes   | Auto-detected delimiter                                                           |
| JSON/JSON Lines           | yes  | yes   | Collapsible JSON Tree view with inline key / value editing.                       |
| Excel                     | yes  | yes   | Opens every sheet as a tab.                                                       |
| ODS                       | yes  | yes   |                                                                                   |
| Arrow IPC / Feather       | yes  | yes   |                                                                                   |
| Avro                      | yes  | yes   |                                                                                   |
| ORC                       | yes  | yes   |                                                                                   |
| HDF5                      | yes  | no    |                                                                                   |
| NetCDF v3 (.nc)           | yes  | no    |                                                                                   |
| SQLite                    | yes  | yes   | Multi-table picker; diff-based writes via rowid identity.                         |
| DuckDB                    | yes  | yes   | Multi-table picker; SQL Query view exposes the file as `data`.                    |
| GeoPackage (.gpkg)        | yes  | yes   | Multi-table picker.                                                               |
| SAS (.sas7bdat)           | yes  | no    |                                                                                   |
| SPSS (.sav, .zsav)        | yes  | yes   |                                                                                   |
| Stata (.dta)              | yes  | yes   |                                                                                   |
| R (.rds, .rdata)          | yes  | no    |                                                                                   |
| DBF/dBase (.dbf)          | yes  | yes   |                                                                                   |
| XML                       | yes  | yes   |                                                                                   |
| TOML                      | yes  | yes   |                                                                                   |
| YAML                      | yes  | yes   | Collapsible YAML Tree view (mirrors JSON Tree).                                   |
| Jupyter Notebook          | yes  | yes   | Notebook view renders code + markdown cells with syntect highlighting.            |
| Markdown                  | yes  | yes   | Rendered preview, Split, and Edit modes.                                          |
| EPUB                      | yes  | no    | EPUB Reader view, chapter-by-chapter with embedded images.                        |
| GeoJSON (.geojson)        | yes  | no    | Map view with OSM tile rendering or geometry-only fallback.                       |
| Archive (zip / tar / tgz) | yes  | no    | Read-only listing; per-entry extract-and-open action.                             |
| Fixed-width (FWF)         | yes  | no    | `.fwf` / `.prn`; read-only, best-effort column-boundary inference.                |
| Source code / config      | yes  | yes   | `.py`, `.rs`, `.go`, `.ts`, ... opened with syntect highlighting in the Raw view. |
| Plain Text                | yes  | yes   |                                                                                   |

Unknown file extensions are opened as plain text.

## Features

Most behaviour is configurable, and the
[documentation](https://thorstenfoltz.github.io/octa/) covers every option,
default, and keyboard shortcut in detail.

### Table View

- Virtual table rendering with smooth scrolling for large datasets
- Lazy row loading for Parquet files, handling millions of rows
- Inline cell editing with type-aware parsing
- Column resize, drag-and-drop reorder, and double-click best-fit width
- Ascending/descending sort by any column
- Cell, row, and column selection with clipboard copy/paste
- Search and filter across all columns in real time (Plain / Wildcard / Regex modes), with match-case and whole-word toggles, a single-column scope, and a persistent **Recent** history
- Excel-style formulas in cells (`=A1+B1`) and as the "Insert column" formula
- Thousand separators for numeric cells (English / European styles) plus per-column rounding, all display-only and never written to saved data

### Multiple View Modes

- **Table** — structured spreadsheet display (default)
- **Raw Text** — source text with line numbers and optional column alignment.
Syntect-based syntax highlighting kicks in for languages with no dedicated
view (Python, Rust, shell, Terraform, etc.)
- **Markdown** — rendered CommonMark preview with Preview / Split / Edit toggle; Split places a TextEdit next to a live preview.
- **JSON Tree** / **YAML Tree** — collapsible Firefox-style tree for `.json` / `.jsonl` / `.yaml` / `.yml`. Keys are renamable, values are editable, and you can add keys to objects in place.
- **Notebook** — rendered Jupyter notebook with code cells, markdown cells, and outputs.
- **EPUB Reader** — chapter-by-chapter rendered text for `.epub` files. Top toolbar shows the book title, Previous/Next, and a chapter combo. Embedded images render as a thumbnail strip below the chapter body.
- **Map** — slippy-map view for `.geojson` files. OSM tiles (configurable URL) with feature geometries painted on top. Toolbar toggles Tiles ↔ Geometry-only; plain mouse-wheel zoom; double-click to zoom in.
- **Compare** — side-by-side comparison of two files. Four sub-modes toggle in
the Compare toolbar: **Text Diff** (git-style line diff), **Row Hash Diff**
(BLAKE3-hashed columns; uniques + shared rows bucketed), **Ordered** (rows lined
up positionally with the exact changed cells), and **Join** (rows matched on a
key column into added / removed / changed). Cross-format works since hashing
sees cell text only.
- **SQL Query** — write a query against the current table (exposed as `data`) and see results beneath. Line numbers, chip-style autocomplete, UPPER/lower case conversion, a per-tab query **History**, a saved-**Snippets** library, and a brief green highlight of the cells a mutation changed.

### Editing

- Insert, delete, and move rows and columns
- Colour marking for cells, rows, and columns with six colour choices
- Conditional formatting: rule-based automatic cell colouring (equals, contains, greater-than, is empty, ...) that applies live
- Data validation: flag cells that break a rule (not empty, in range, matches a pattern, unique, max length) — failing cells are highlighted red
- Multi-column sort (**Analyse → Sort by columns...**): sort by an ordered list of columns, each ascending or descending
- Copy the current selection as a GitHub-flavoured Markdown table
- Undo / Redo for cell edits, structural changes, and colour marks
- Leading/trailing whitespace trimmed from string cells and column titles on load (configurable, with a banner listing the affected columns)
- Unsaved-changes guards on close and file open
- Save in the original format or export to a different one via Save As
- Reopen Last Closed Tab (default **Ctrl+Shift+T**) restores accidentally-closed tabs
- Find duplicates (default **Ctrl+Shift+D**) picks dedupe-key columns and either highlights duplicate rows or opens them in a new tab

### Inspecting data

- **Summary** — one row of statistics per column (min, max, sum, mean, median, standard deviation, range, IQR, quartiles, mode and its count, null counts, exact unique count, distinct ratio, text length, total rows). Headers are short `snake_case` identifiers so the table is easy to reuse, with the localised description on hover; you choose which statistics appear under **Settings → Summary**
- **Value Frequency** — `value_counts()`-style top-N values for any column.
Numeric columns can be turned into a histogram: type a bin count (or leave it for automatic Sturges binning) and get that many equal-width ranges with their counts
- **Pivot / Unpivot** — reshape a table between long and wide form (DuckDB `PIVOT` / `UNPIVOT`) into a new tab
- **Schema Export** — render the column list as Postgres / MySQL / SQLite / Databricks / Snowflake DDL, Pydantic v2, TypeScript interface, JSON Schema, or a Rust struct. Also available from the CLI (`octa --export-schema`) and over MCP.
- **Chart** — open a new tab plotting the active table as a histogram, bar, line, scatter, or box chart via `egui_plot`.
Customisable title / axis / legend / per-series colour, PNG/SVG/PDF export, log scale.

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
octa --schema data.parquet                   # print column schema
octa --head data.csv -n 5                    # first N rows (default 20)
octa --head data.csv -n 5 -f json            # output as JSON instead of TSV
octa --convert in.csv out.parquet            # convert formats
octa --sql data.parquet -q 'SELECT count(*) FROM data'
octa --export-schema data.parquet -t snowflake   # schema as DDL / model / struct
```

Output format is selectable with `-f / --format {tsv|json|csv}` (TSV default). Run `octa --help` for the full reference, and see the [CLI docs](https://thorstenfoltz.github.io/octa/cli/) for every action.

### MCP server

`octa --mcp` starts a [Model Context Protocol](https://modelcontextprotocol.io/) server on stdio that exposes Octa's reading, inspection, and write capabilities as MCP tools. Point any MCP client (Claude Desktop, Claude Code,
MCP Inspector, or any compatible client) at the same binary and the model can query your local data files directly, no scripting in between. Add `--mcp-read-only` to drop the data-writing tools for clients that should only ever read.

### Assistant (in-app chat)

A docked chat panel where you ask an LLM about your data in plain language and
it answers by driving Octa's own tools against the tabs you already
have open. It is the in-application sibling of the MCP server, with no external
client to set up, and it can save results back to a file.

It is local-first and provider-agnostic: use a cloud model (Anthropic Claude,
OpenAI, Google Gemini, or any OpenAI-compatible endpoint such as OpenRouter /
Groq / LM Studio) or run fully offline with [Ollama](https://ollama.com/). API
keys are kept in your OS keyring, reads are sandboxed
to the files you have open, and writes go to a configurable export directory.

A **Prompts** library lets you save and reuse common requests, and an optional
tool-call audit log records every tool the assistant runs (metadata only, never
cell contents) for review.

### Settings

- Configurable font size and theme
- Font picker for the SQL editor
- Performance knobs such as streaming row caps and size limits
- User-extensible "open as plain text" extension list
- Directory sidebar filter to list only files Octa can open (on by default)
- Remappable keyboard shortcuts

### Other

- CSV delimiter auto-detection (comma, semicolon, pipe, tab) and manual selection
- Date inference for text-formatted columns (CSV, JSON, Excel, etc.) with an ambiguity picker for European vs US-format dates
- Auto-update check from GitHub releases
- Cross-platform: Linux, macOS, and Windows

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

The same `Dockerfile` works with Podman (swap `docker` for `podman`).

## Installation

### Linux (and WSL)

The quickest way to install is a single command that downloads `get-octa.sh`
and runs it. The script fetches the latest release from GitHub, extracts it,
and installs the binary, icon, desktop entry, and man page.

User-local install with curl (no sudo needed, installs to `~/.local`):

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash -s -- ~/.local
```

System-wide install with curl (installs to `/usr/local`):

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | sudo bash
```

The same with wget, user-local:

```bash
wget -qO- https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash -s -- ~/.local
```

And wget, system-wide:

```bash
wget -qO- https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | sudo bash
```

After a user-local install, make sure `~/.local/bin` is on your `PATH` (it
usually is on Ubuntu and most other distributions). If `octa` is not found,
add `export PATH="$HOME/.local/bin:$PATH"` to your `~/.bashrc` or `~/.zshrc`.

If `octa` is still "command not found" right after installing even though it
landed in a directory already on your `PATH` (e.g. `/usr/local/bin`), your
running shell has a stale command-lookup cache. Refresh it with `hash -r`
(bash) or `rehash` (zsh), or just open a new shell. That is why a shell
restart "fixes" the install.

On WSL you may see harmless warnings on launch such as:

```
libEGL warning: failed to get driver name for fd -1
MESA: error: ZINK: failed to choose pdev
libEGL warning: failed to create dri2 screen
```

These come from Mesa, not Octa: WSL has no native OpenGL GPU, so Mesa fails to
initialise its Zink/DRI hardware path and falls back to software rendering
(llvmpipe). Octa still runs correctly and fast. To silence the noise, force
software rendering up front:

```bash
LIBGL_ALWAYS_SOFTWARE=1 octa file.parquet
# or make it permanent for the shell:
echo 'export LIBGL_ALWAYS_SOFTWARE=1' >> ~/.bashrc   # or ~/.zshrc
```

Prefer not to pipe a script? Download the Linux tarball from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and extract it:

```bash
tar xzf octa-*-linux-x86_64.tar.gz
```

Run the binary in place without installing:

```bash
./octa-*-linux-x86_64/octa
```

Or install it system-wide from the extracted directory:

```bash
sudo ./octa-*-linux-x86_64/install.sh
```

### Linux AppImage

An [AppImage](https://appimage.org/) is published alongside each release
for users who prefer a single portable file. Download `Octa-*-x86_64.AppImage`
from the [releases page](https://github.com/thorstenfoltz/octa/releases) and
make it executable:

```bash
chmod +x Octa-*-x86_64.AppImage
```

Then run it directly:

```bash
./Octa-*-x86_64.AppImage
```

See [Installation](https://thorstenfoltz.github.io/octa/getting-started/installation/)
for the FUSE-less AppImage fallback and other options.

### Linux (build from source)

Install the Rust toolchain from <https://rustup.rs/> and the native
libraries (GTK 3, fontconfig, freetype, libssl, libxcb, libxkbcommon),
then clone the repository:

```bash
git clone https://github.com/thorstenfoltz/octa.git && cd octa
```

Build a release binary:

```bash
cargo build --release
```

Then install the freshly built binary with the local `install.sh`:

```bash
./install.sh ~/.local
```

Full dependency list and build notes are in the
[documentation](https://thorstenfoltz.github.io/octa/getting-started/installation/).

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

The simplest option is to download `octa.exe` from the
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
[releases page](https://github.com/thorstenfoltz/octa/releases). The build
targets Apple Silicon (`aarch64`) Macs. Drop `Octa.app` into
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
