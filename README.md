# Octa

<p align="left">
  <img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

An application for viewing data files. Octa opens files in a spreadsheet-like table view with sorting, filtering, and search options. Writing is supported but limited. Octa is primarily a reader.

## Supported Formats

| Format              | Read | Write |
|---------------------|------|-------|
| Parquet             | yes  | yes   |
| CSV / TSV           | yes  | yes   |
| JSON / JSON Lines   | yes  | yes   |
| Excel               | yes  | yes   |
| Arrow IPC / Feather | yes  | yes   |
| Avro                | yes  | yes   |
| ORC                 | yes  | yes   |
| HDF5                | yes  | no    |
| SQLite              | yes  | yes   |
| DuckDB              | yes  | yes   |
| SAS (.sas7bdat)     | yes  | no    |
| SPSS (.sav, .zsav)  | yes  | yes   |
| Stata (.dta)        | yes  | yes   |
| R (.rds)            | yes  | no    |
| DBF / dBase (.dbf)  | yes  | yes   |
| XML                 | yes  | yes   |
| TOML                | yes  | yes   |
| YAML                | yes  | yes   |
| PDF                 | yes  | yes   |
| Jupyter Notebook    | yes  | yes   |
| Markdown            | yes  | yes   |
| Plain Text          | yes  | yes   |

Unknown file extensions are opened as plain text.

## Features

### Table View

- Virtual table rendering with smooth scrolling for large datasets
- Lazy row loading for Parquet files (handles millions of rows)
- Inline cell editing with type-aware parsing
- Column resize and drag-and-drop reorder
- Ascending/descending sort by any column
- Cell, row, and column selection with clipboard copy/paste
- Search and filter across all columns in real time

### Multiple View Modes

- **Table View** -- structured spreadsheet display (default)
- **Raw Text View** -- source text with line numbers and optional column alignment
- **Markdown View** -- rendered CommonMark preview
- **Notebook View** -- rendered Jupyter notebook with code cells, markdown cells, and outputs
- **PDF View** -- page-by-page rendered output
- **SQL Query View** -- write a query against the current table (exposed as `data`) and see results beneath

### Editing

- Insert, delete, and move rows and columns
- Color marking for cells, rows, and columns with six color choices
- Unsaved-changes guards on close and file open
- Save in the original format or export to a different one via Save As

### Settings

- Configurable font size
- Light and dark theme with the option to set a default

### Other

- CSV delimiter auto-detection (comma, semicolon, pipe, tab) and manual selection
- Auto-update check from GitHub releases
- Cross-platform: Linux, macOS, and Windows

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

Building from source additionally requires a C compiler and the native
libraries listed in `CLAUDE.md` (GTK, fontconfig, mupdf, etc.).

### Arch Linux

Available on the AUR as `octa` (build from source) and `octa-bin` (prebuilt binary).

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
xattr -d com.apple.quarantine /Applications/Octa.app
```

To build from source, install the Rust toolchain (<https://rustup.rs/>)
and the native dependencies via Homebrew, then `cargo build --release`:

```bash
brew install mupdf-tools harfbuzz freetype jpeg openjpeg gtk+3
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
