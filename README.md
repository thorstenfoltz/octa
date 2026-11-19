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

**Octa is a desktop app for opening, cleaning and querying data, whatever format
it comes in.** Parquet, CSV, Excel, a SQLite file, a PDF's tables, a mailbox, a
live PostgreSQL database or a bucket on S3: Octa shows it in one fast table
where you can search, edit, compare, analyse and run SQL on it, then save it
back or convert it to another format.

It is a single native program for Linux, macOS and Windows that runs locally
and needs no account. Its engine also works as a command-line tool and as an
MCP server, so AI agents like Claude can use it too.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

![Octa main window](docs/assets/screenshots/hero-table-view.png)

## What it does

- **[Open](https://thorstenfoltz.github.io/octa/guide/open/)** files bigger
  than memory, password-protected files and archives, with tabs, tiling and a
  folder sidebar, plus [views](https://thorstenfoltz.github.io/octa/guide/view/)
  for raw text, JSON trees, maps and timelines.
- **[Connect](https://thorstenfoltz.github.io/octa/guide/connect/)** to S3,
  Azure, GCS, SFTP, WebDAV, twelve database engines and REST APIs.
- **[Edit](https://thorstenfoltz.github.io/octa/guide/edit/)** cells, rows and
  columns with undo, formulas and calculated columns, then
  **[clean](https://thorstenfoltz.github.io/octa/guide/clean/)** duplicates,
  outliers, missing values and personal data.
- **[Combine](https://thorstenfoltz.github.io/octa/guide/combine/)** and
  **[compare](https://thorstenfoltz.github.io/octa/guide/compare/)** tables:
  joins (fuzzy and spatial too), file and database diffs, schema drift, cell
  history in Git.
- **[Analyse](https://thorstenfoltz.github.io/octa/guide/analyse/)** with
  summaries, pivots, correlations, time series, cohorts, charts, forecasts and
  HTML reports.
- **[Query](https://thorstenfoltz.github.io/octa/guide/sql-assistant/)** any
  open table with DuckDB SQL, or ask the built-in assistant, on a cloud model or
  offline with [Ollama](https://ollama.com/).
- **[Save](https://thorstenfoltz.github.io/octa/guide/save/)** in the original
  format or convert to another one, and
  **[automate](https://thorstenfoltz.github.io/octa/guide/automate/)** it all
  with recipes, the [command line](https://thorstenfoltz.github.io/octa/cli/)
  or the [MCP server](https://thorstenfoltz.github.io/octa/mcp/).

## What it reads

Parquet, CSV/TSV, JSON and JSON Lines, Excel, ODS, Arrow, Avro, ORC, SQLite,
DuckDB, GeoPackage, Microsoft Access, SAS, QlikView QVD, SPSS, Stata, R, HDF5,
MATLAB, NetCDF, NumPy, MessagePack, BSON, DBF, XML, TOML, YAML, Jupyter
notebooks, Markdown, HTML, EPUB, PDF tables, log files, GeoJSON, Shapefile, GPX,
KML, Delta Lake, Apache Iceberg, Lance, fixed-width text, zip/tar archives, SQL
dumps, email (`.eml`, `.mbox`, Outlook `.msg`, `.pst`, `.olm`), calendars and
contacts, and source code. See
[supported formats](https://thorstenfoltz.github.io/octa/getting-started/supported-formats/)
for what each one can do, and [`samples/`](samples/) for an example of each.

## Installation

Building from source works on all three platforms: install
[Rust](https://rustup.rs/), clone this repository, and run
`cargo build --release`. Linux and macOS need a few native libraries first,
Windows needs a C++ toolchain; the
[installation guide](https://thorstenfoltz.github.io/octa/getting-started/installation/)
lists them per platform, along with uninstall steps and the first-launch
security prompts on Windows and macOS.

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

## License

[MIT](LICENSE)
