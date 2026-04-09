# Octa

<p align="left">
  <img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

An application for viewing data files. Octa opens files in a spreadsheet-like table view with sorting, filtering, and search. Writing is supported but limited. Octa is primarily a reader.

## Supported Formats

| Format              | Read | Write |
|---------------------|------|-------|
| Parquet             | yes  | yes   |
| CSV / TSV           | yes  | yes   |
| JSON / JSON Lines   | yes  | yes   |
| Excel               | yes  | yes   |
| Arrow IPC / Feather | yes  | yes   |
| Avro                | yes  | yes   |
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
- Cross-platform: Linux and Windows

## Screenshots

<p align="left">
  <img src="assets/octa-rose.png" alt="Octa Icon" width="128" height="128">
</p>

## Installation

### Arch Linux

Available on the AUR as `octa` (build from source) and `octa-bin` (prebuilt binary).

```bash
paru -S octa
```

or

```bash
paru -S octa-bin
```

### Other Linux Distros

Clone the repository and execute the installation script.

```bash
# System-wide (installs to /usr/local)
sudo ./install.sh

# User-local (no sudo needed)
./install.sh ~/.local
```

This installs the binary, SVG icon, and desktop entry so Octa appears in your application launcher with file associations for all supported formats.

### Windows

Run `install.bat` as Administrator. If it doesn't work, just copy and paste the exe file to the wished location. Please note, Defender will probably prevent
opening, because of an unknown publisher. Expand the warning completely and accept, then it works.

## Configuration

Settings are stored in:

- **Linux:** `$XDG_CONFIG_HOME/octa/settings.toml` (defaults to `~/.config/octa/settings.toml`)
- **Windows:** `%APPDATA%\Octa\settings.toml`

## License

MIT
