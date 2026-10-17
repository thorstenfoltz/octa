# Octa

<p align="left">
<img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

[![MegaLinter](https://github.com/thorstenfoltz/octa/actions/workflows/megalinter.yml/badge.svg)](https://github.com/thorstenfoltz/octa/actions/workflows/megalinter.yml)
[![Docs](https://github.com/thorstenfoltz/octa/actions/workflows/docs.yml/badge.svg)](https://thorstenfoltz.github.io/octa/)
[![Release](https://img.shields.io/github/v/release/thorstenfoltz/octa)](https://github.com/thorstenfoltz/octa/releases/latest)
[![AUR](https://img.shields.io/aur/version/octa)](https://aur.archlinux.org/packages/octa)
[![Microsoft Store](https://img.shields.io/badge/Microsoft%20Store-Octa-0078D4)](https://apps.microsoft.com/detail/9PF9BVRT9PX4)
[![Docker](https://img.shields.io/badge/ghcr.io-thorstenfoltz%2Focta-2496ED)](https://github.com/thorstenfoltz/octa/pkgs/container/octa)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A native desktop application for viewing, editing, and querying data files. Octa
opens a file in a spreadsheet-like table with sorting, filtering, and search, and
runs as a single binary on Linux, macOS, and Windows.

Files are not read-only: edit cells, rows and columns with undo, then save in
the original format or convert to another one with Save As. Any open table, a
plain CSV included, can also be queried with SQL through DuckDB.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

![Open a file](docs/assets/screenshots/first-steps-file-menu.png)

## Contents

- [What it does](#what-it-does)
- [What it reads](#what-it-reads)
- [Installation](#installation)
  - [Arch Linux](#arch-linux)
  - [Linux (and WSL)](#linux-and-wsl)
  - [Docker](#docker)
  - [Windows](#windows)
  - [macOS](#macos)
- [License](#license)

## What it does

The [user guide](https://thorstenfoltz.github.io/octa/guide/) is sorted by task, and each topic lists where a
job lives: in the app, on the command line, and as an MCP tool for AI agents.

- **[Open files](https://thorstenfoltz.github.io/octa/guide/open/)**: tabs, a folder sidebar, files bigger than
  memory, password-protected files, archives, PDF tables and log files.
- **[Connect](https://thorstenfoltz.github.io/octa/guide/connect/)**: S3, Azure, GCS, SFTP and WebDAV, twelve
  live database engines from PostgreSQL to BigQuery, and REST APIs.
- **[View and find](https://thorstenfoltz.github.io/octa/guide/view/)**: sorting, search, filters, marks,
  bookmarks, and views for raw text, JSON trees, maps and timelines.
- **[Edit and reshape](https://thorstenfoltz.github.io/octa/guide/edit/)**: inline edits with undo, formulas,
  type changes, pivot and transpose.
- **[Clean and protect](https://thorstenfoltz.github.io/octa/guide/clean/)**: duplicates, outliers, missing
  values, validation rules, personal data and anonymisation.
- **[Combine and split](https://thorstenfoltz.github.io/octa/guide/combine/)**: union, join, fuzzy and spatial
  joins, key finding, merging edited copies, splitting into files.
- **[Analyse](https://thorstenfoltz.github.io/octa/guide/analyse/)**: summaries, value counts, correlations,
  time series, charts, forecasts and HTML reports.
- **[Compare and history](https://thorstenfoltz.github.io/octa/guide/compare/)**: file and database diffs,
  schema and data drift, cell history in Git, earlier versions.
- **[Save and export](https://thorstenfoltz.github.io/octa/guide/save/)**: save back, convert, PDF, workbooks
  and DDL.
- **[SQL and assistant](https://thorstenfoltz.github.io/octa/guide/sql-assistant/)**: DuckDB SQL over any open
  file, and a chat assistant that runs on a cloud model or offline with
  [Ollama](https://ollama.com/).
- **[Automate](https://thorstenfoltz.github.io/octa/guide/automate/)**: recipes that replay a clean-up, batch
  conversion, a full [command line](https://thorstenfoltz.github.io/octa/cli/) and an
  [MCP server](https://thorstenfoltz.github.io/octa/mcp/) for Claude and other
  [Model Context Protocol](https://modelcontextprotocol.io/) clients.

## What it reads

Parquet, CSV/TSV, JSON and JSON Lines, Excel, ODS, Arrow, Avro, ORC, SQLite,
DuckDB, GeoPackage, SAS, SPSS, Stata, R, HDF5, NetCDF, NumPy, MessagePack, BSON,
DBF, XML, TOML, YAML, Jupyter notebooks, Markdown, HTML, EPUB, PDF tables, log
files, GeoJSON, Shapefile, Delta Lake, Apache Iceberg, fixed-width text, zip/tar
archives, SQL dumps, and source code. Most of those can be written back, and
Save As converts between them.

See [supported formats](https://thorstenfoltz.github.io/octa/getting-started/supported-formats/)
for the full list and what each one can do, or open [`samples/`](samples/) for
one small example of every one of them.

## Installation

Building from source works on all three platforms: install
[Rust](https://rustup.rs/), clone this repository, and run
`cargo build --release`. Linux and macOS need a few native libraries first,
Windows needs a C++ toolchain; the
[installation guide](https://thorstenfoltz.github.io/octa/getting-started/installation/)
lists them per platform, along with uninstall steps and the first-launch
security prompts on Windows and macOS.

### Arch Linux

On Arch and Arch-based distributions (Manjaro, EndeavourOS, Garuda, ...) Octa is
on the AUR, either built from source or as the prebuilt binary. Both install the
man page, so `man octa` works:

Build from source:

```bash
paru -S octa
```

Or take the prebuilt binary:

```bash
paru -S octa-bin
```

Swap `paru` for `yay` or any other AUR helper you use.

### Linux (and WSL)

Install the latest release with one command, user-local (no sudo):

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash -s -- ~/.local
```

Or system-wide into `/usr/local`:

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | sudo bash
```

On WSL, also install a file dialog backend, or **Open** and **Save as** have
nothing to show and appear to do nothing (Octa says so in the status bar):

```bash
sudo apt install xdg-desktop-portal-gtk   # or: sudo apt install zenity
```

An AppImage and a plain tarball are on the
[releases page](https://github.com/thorstenfoltz/octa/releases). A downloaded
AppImage still needs its execute bit, and a file manager will silently ignore a
double-click until it has one:

```bash
chmod 750 Octa-*-x86_64.AppImage
./Octa-*-x86_64.AppImage
```

### Docker

A headless image ships the CLI and the MCP server:

```bash
docker pull ghcr.io/thorstenfoltz/octa:latest
```

Mount a directory and pass any CLI flag:

```bash
docker run --rm -v "$PWD:/data" ghcr.io/thorstenfoltz/octa --schema /data/file.parquet
```

See the [container docs](https://thorstenfoltz.github.io/octa/cli/docker/) for
the MCP invocation and Podman.

### Windows

Install from the
[Microsoft Store](https://apps.microsoft.com/detail/9PF9BVRT9PX4): one click, no
SmartScreen prompt, and the Store keeps it updated.

Or download `octa.exe` from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and run it, no
installation needed. `install.ps1` (per-user, no admin) or `install.bat`
(system-wide) set it up properly with a Start Menu entry.

Those downloads are not code-signed, so Windows shows *"Windows protected your
PC"* on first launch. It is safe to continue:
[how to get past the SmartScreen prompt](https://thorstenfoltz.github.io/octa/troubleshooting/#windows-smartscreen-prompt).

### macOS

One line, no password, Apple Silicon:

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash
```

It puts `Octa.app` into `~/Applications` and `octa` into `~/.local/bin`, and
because `curl` marks nothing as quarantined, macOS opens it without a
Gatekeeper warning. Intel Macs build from source.

The `Octa.app` bundle is also on the
[releases page](https://github.com/thorstenfoltz/octa/releases). It is not
notarised, so a copy downloaded with a browser makes macOS claim *"the developer
cannot be verified"* (and occasionally that the app is *damaged*). Neither means
anything is wrong with the download:
[how to open it anyway](https://thorstenfoltz.github.io/octa/troubleshooting/#macos-developer-cannot-be-verified).

## License

MIT
