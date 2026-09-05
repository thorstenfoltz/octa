# Octa

<p align="left">
<img src="assets/octa-rose.svg" alt="Octa" width="128" height="128">
</p>

A native desktop application for viewing, editing, and querying data files. Octa
opens a file in a spreadsheet-like table with sorting, filtering, and search, and
runs as a single binary on Linux, macOS, and Windows.

Files are not read-only. Edit cells in place, insert or delete rows and columns,
find and replace, transform a column, colour-mark cells or flag the ones that
break a validation rule, all with undo, then save in the original format or
convert to another one with Save As.

Any open table, a plain CSV included, can also be reshaped with SQL. The SQL
panel exposes the current file as `data` and runs your query through DuckDB, so
you can aggregate it, join it against another file, and keep the result as a new
tab or write it straight to a file or a database.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

![Open a file](docs/assets/screenshots/first-steps-file-menu.png)

## Contents

- [What it reads](#what-it-reads)
- [Beyond local files](#beyond-local-files)
- [Installation](#installation)
  - [Arch Linux](#arch-linux)
  - [Linux (and WSL)](#linux-and-wsl)
  - [Docker](#docker)
  - [Windows](#windows)
  - [macOS](#macos)
- [License](#license)

## What it reads

Parquet, CSV/TSV, JSON and JSON Lines, Excel, ODS, Arrow, Avro, ORC, SQLite,
DuckDB, GeoPackage, SAS, SPSS, Stata, R, HDF5, NetCDF, NumPy, MessagePack, BSON,
DBF, XML, TOML, YAML, Jupyter notebooks, Markdown, HTML, EPUB, GeoJSON,
Shapefile, Delta Lake, Apache Iceberg, fixed-width text, zip/tar archives, SQL
dumps, and source code.
Most of those can be written back, and Save As converts between them.

See [supported formats](https://thorstenfoltz.github.io/octa/getting-started/supported-formats/)
for the full list and what each one can do, or open [`samples/`](samples/) for
one small example of every one of them.

## Beyond local files

- **Cloud object storage.** Browse and open objects straight from Amazon S3 (and
  S3-compatible providers), Azure Blob Storage, and Google Cloud Storage. Saving
  back is opt-in.
- **Live databases.** Connect to twelve SQL engines and cloud warehouses:
  PostgreSQL, MySQL/MariaDB, SQL Server, Oracle, Amazon Redshift, ClickHouse,
  Exasol, Trino, Amazon Athena, Snowflake, Databricks, and Google BigQuery. Browse tables, query them in their
  own dialect, join them against local files, and (when you opt in) write edits
  back.
- **MCP server.** `octa --mcp` speaks the
  [Model Context Protocol](https://modelcontextprotocol.io/) over stdio, so any
  MCP client can read and analyse your data through Octa instead of a custom
  script.
- **Built-in chat assistant.** A docked panel where an LLM answers questions
  about your open tabs by driving Octa's own tools. Use a cloud model or run
  fully offline with [Ollama](https://ollama.com/).
- **Command line.** The same binary is a CLI: `octa --schema`, `--head`,
  `--convert`, `--sql`, `--export-schema`, and more. `octa --help` lists them
  all. Shell completions come with it:
  `eval "$(octa --completions zsh)"`, or let `install.sh` write the files.

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

Download the `Octa.app` bundle for Apple Silicon from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and drop it into
`/Applications`. Intel Macs build from source.

The app is not signed or notarised, so macOS claims *"the developer cannot be
verified"* (and occasionally that the app is *damaged*). Neither means anything
is wrong with the download:
[how to open it anyway](https://thorstenfoltz.github.io/octa/troubleshooting/#macos-developer-cannot-be-verified).

## License

MIT
