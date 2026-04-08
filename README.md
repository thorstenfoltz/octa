# Octa

<p align="left">
  <img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

A native desktop application for viewing and editing data files, built with Rust and [egui](https://github.com/emilk/egui). Octa opens files in a spreadsheet-like table view and supports inline editing, sorting, filtering, undo/redo, color marking, and more.

## Supported Formats

| Format              | Read | Write |
|---------------------|------|-------|
| Parquet             | yes  | yes   |
| CSV / TSV           | yes  | yes   |
| JSON / JSON Lines   | yes  | yes   |
| Excel (xlsx)        | yes  | yes   |
| Arrow IPC / Feather | yes  | yes   |
| Avro                | yes  | yes   |
| XML                 | yes  | yes   |
| TOML                | yes  | yes   |
| YAML                | yes  | yes   |
| PDF                 | yes  | yes   |
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
- **PDF View** -- page-by-page rendered output

### Editing

- Full undo/redo support (Ctrl+Z / Ctrl+Y)
- Insert, delete, and move rows and columns
- Color marking for cells, rows, and columns with six color choices
- Unsaved-changes guards on close and file open
- Save in the original format or export to a different one via Save As

### Settings

- Configurable font size
- Light and dark theme with the option to set a default
- Customizable icon color (Rose, Amber, Blue, Cyan, Emerald, Indigo, Lime, Orange, Purple, Red, Slate, Teal)
- Settings persist across sessions

### Other

- CSV delimiter auto-detection (comma, semicolon, pipe, tab) and manual selection
- Auto-update check from GitHub releases
- Cross-platform: Linux and Windows

## Screenshots

<p align="left">
  <img src="assets/octa-rose.png" alt="Octa Icon" width="128" height="128">
</p>

## Building

Requires Rust (edition 2024) and a C compiler (for native dependencies like mupdf).

```bash
# Debug build
cargo build

# Release build (optimized, stripped)
cargo build --release

# Run directly
cargo run

# Run with a file
cargo run -- path/to/file.parquet
```

### System Dependencies (Linux)

```bash
sudo apt-get install -y libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libfontconfig1-dev \
  libfreetype6-dev libharfbuzz-dev libfribidi-dev libjpeg-dev \
  libopenjp2-7-dev libgumbo-dev libjbig2dec0-dev libmujs-dev
```

## Installation

### Arch Linux

A `PKGBUILD` is available in `.github/aur/` for building an Arch Linux package.

### Other Linux Distros

```bash
# System-wide (installs to /usr/local)
sudo ./install.sh

# User-local (no sudo needed)
./install.sh ~/.local
```

This installs the binary, SVG icon, and desktop entry so Octa appears in your application launcher with file associations for all supported formats.

### Windows

Run `install.bat` as Administrator. This builds a release binary, copies it to `%ProgramFiles%\Octa`, adds it to your PATH, and creates a Start Menu shortcut.

## Testing

```bash
# Run all tests
cargo test

# Run a single test
cargo test <test_name>
```

## Configuration

Settings are stored in:

- **Linux:** `$XDG_CONFIG_HOME/octa/settings.toml` (defaults to `~/.config/octa/settings.toml`)
- **Windows:** `%APPDATA%\Octa\settings.toml`

Available settings:

| Setting         | Default | Description                        |
|-----------------|---------|------------------------------------|
| `font_size`     | `13.0`  | Base font size in points (8 -- 32) |
| `default_theme` | `Light` | Startup theme (`Light` or `Dark`)  |
| `icon_variant`  | `Rose`  | Icon color variant                 |

Settings can also be changed from **Help > Settings** inside the application.

## License

MIT
