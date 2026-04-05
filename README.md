# Octo

<p align="left">
  <img src="assets/octo.svg" alt="Octo" width="128" height="128">
</p>

A native desktop application for viewing and editing data files. The focus is on opening files in table format.

## Supported Formats

| Format              | Read | Write |
|---------------------|------|-------|
| parquet             | yes  | yes   |
| csv / tsv           | yes  | yes   |
| json / json lines   | yes  | yes   |
| excel               | yes  | yes   |
| arrow ipc / feather | yes  | yes   |
| avro                | yes  | yes   |
| xml                 | yes  | yes   |
| toml                | yes  | yes   |
| yaml                | yes  | yes   |
| pdf                 | yes  | yes   |
| markdown            | yes  | yes   |
| plain text          | yes  | yes   |

Unknown file extensions are opened as plain text.

## Features

- Virtual table rendering with smooth scrolling for large datasets
- Lazy row loading for Parquet files (millions of rows)
- Inline cell editing with type-aware parsing
- Column resize, drag-and-drop reorder, and sorting
- Cell, row, and column selection with copy/paste
- Color marking for cells, rows, and columns
- Undo/redo support (Ctrl+Z / Ctrl+Y)
- Row and column insert, delete, and move operations
- Search/filter across all columns
- Raw text view with line numbers for text-based formats
- Rendered Markdown view with CommonMark support
- PDF page rendering
- Dark and light themes
- CSV delimiter auto-detection and manual selection

## Building

Requires Rust 1.70+ and a C compiler (for native dependencies like mupdf).

```bash
# Debug build
cargo build
```

### Release build (optimized, stripped)

```bash
cargo build --release
```

### Run directly

``` bash
cargo run
```

## Installation

### Arch Linux

A `PKGBUILD` is available in `.github/aur/` for building an Arch Linux package.

### Other Distros

```bash
# System-wide (installs to /usr/local)
sudo ./install.sh

# User-local (no sudo needed)
./install.sh ~/.local
```

This installs the binary, SVG icon, and desktop entry so Octo appears in your application launcher with file associations.

### Windows

Run `install.bat` as Administrator. This builds a release binary, copies it to `%ProgramFiles%\Octo`, adds it to your PATH, and creates a Start Menu shortcut.

## Testing

```bash
cargo test
```

## License

MIT
