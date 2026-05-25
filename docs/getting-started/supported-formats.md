# Supported Formats

Octa reads ~25 file formats out of the box. Most are also writable.
Unknown extensions fall back to the plain-text reader so you can
always open *something*.

## At-a-glance matrix

| Format                        | Extensions                                | Read | Write |
|-------------------------------|-------------------------------------------|:----:|:-----:|
| **Parquet**                   | `.parquet`                                |  ✅   |   ✅   |
| **CSV / TSV**                 | `.csv`, `.tsv`                            |  ✅   |   ✅   |
| **JSON**                      | `.json`                                   |  ✅   |   ✅   |
| **JSON Lines**                | `.jsonl`, `.ndjson`                       |  ✅   |   ✅   |
| **Excel**                     | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.xlm` |  ✅   |  ✅ *  |
| **ODS**                       | `.ods`                                    |  ✅   |   ✅   |
| **Arrow IPC / Feather**       | `.arrow`, `.feather`                      |  ✅   |   ✅   |
| **Avro**                      | `.avro`                                   |  ✅   |   ✅   |
| **ORC**                       | `.orc`                                    |  ✅   |   ✅   |
| **HDF5**                      | `.h5`, `.hdf5`, `.hdf`                    |  ✅   |   ❌   |
| **NetCDF v3**                 | `.nc`                                     |  ✅   |   ❌   |
| **SQLite**                    | `.sqlite`, `.sqlite3`, `.db`              |  ✅   | ✅ **  |
| **DuckDB**                    | `.duckdb`, `.ddb`                         |  ✅   | ✅ **  |
| **GeoPackage**                | `.gpkg`                                   |  ✅   | ✅ **  |
| **SAS**                       | `.sas7bdat`                               |  ✅   |   ❌   |
| **SPSS**                      | `.sav`, `.zsav`                           |  ✅   |   ✅   |
| **Stata**                     | `.dta`                                    |  ✅   |   ✅   |
| **R Datasets**                | `.rds`, `.rdata`, `.rda`                  |  ✅   |   ❌   |
| **DBF / dBase**               | `.dbf`                                    |  ✅   |   ✅   |
| **XML**                       | `.xml`                                    |  ✅   |   ✅   |
| **TOML**                      | `.toml`                                   |  ✅   |   ✅   |
| **YAML**                      | `.yaml`, `.yml`                           |  ✅   |   ✅   |
| **Jupyter notebook**          | `.ipynb`                                  |  ✅   |   ✅   |
| **Markdown**                  | `.md`, `.markdown`, `.mdown`, `.mkd`      |  ✅   |   ✅   |
| **EPUB**                      | `.epub`                                   |  ✅   |   ❌   |
| **GeoJSON**                   | `.geojson`                                |  ✅   |   ❌   |
| **Archive (zip / tar / tgz)** | `.zip`, `.tar`, `.tgz`                    |  ✅   |   ❌   |
| **Plain text**                | anything else                             |  ✅   |   ✅   |

\* **Excel write** always produces `.xlsx` structure, because the
writer uses `rust_xlsxwriter` which doesn't emit legacy `.xls` /
`.xlsm` / `.xlsb`. Save those as `.xlsx` to round-trip them through
Octa.

\*\* **Database writes** are diff-based and reject schema changes.
See [Saving](../usage/saving.md#database-files-sqlite-duckdb-geopackage)
for details.

## Caveats and limitations by format

### Streaming readers (large files OK)

Parquet, CSV, and TSV all stream. Octa loads the first
`AppSettings.initial_load_rows` (default 5,000,000) rows and
continues loading the rest in the background as you scroll. You
can change the cap (or tick the **Unlimited** checkbox to load
every row up front) under
[**Settings → Performance**](../reference/settings.md#performance).
From the CLI, override per-invocation with `--rows N|all`. From
MCP, pass `unlimited: true` to a tool to lift the cap for that
single call. Multi-million-row files open without delay; the bottom
of the table fills in as you reach it.

Parquet files written with very many small row groups
(more than 32,767 — common with Spark or streaming ingest
pipelines) used to fail the native arrow-parquet reader with
`Row group ordinal 32768 exceeds i16 max value`. Octa now retries
those reads through a DuckDB-backed reader automatically — same
schema and types, no user action required.

### R datasets

Octa only handles the **single `data.frame` / `tibble`** case for
`.rds`. Workspace files (`.rdata` / `.rda` produced by `save()`) are
registered by extension but currently return an error pointing you
at `saveRDS()`, since `rds2rust` only accepts the `X\n` magic of
single-object RDS, not the `RDX2\n` workspace envelope.

### HDF5

Octa uses a pure-Rust HDF5 parser (no system libhdf5 dependency).
Compound datasets (the layout pandas/PyTables write for DataFrames)
are decoded field-by-field.

!!! warning "HDF5 1.10+ vs older files"

    The upstream `hdf5-reader 0.2` library misreads **compound v1
    layouts** when members don't start on 8-byte boundaries.
    HDF5 1.10+ files with compound v3 (the default for h5py
    `libver="latest"` and modern pandas) parse correctly. Older
    pandas / pytables files may surface garbled columns.

### NetCDF

Octa supports **NetCDF v3** only. NetCDF v4 files are HDF5 under
the hood, so open them with the [HDF5 reader](#hdf5) by renaming
the extension.

The reader groups all 1D variables sharing the largest dimension into
one table (each variable becomes a column). Multi-dimensional or
scalar variables are skipped, with a count surfaced in the file's
format label (e.g. *"NetCDF (3 multi-D vars skipped)"*).

### EPUB

Read-only. Octa converts each chapter's XHTML to Markdown at load
time and renders chapter-by-chapter in the
[EPUB Reader view](../usage/view-modes/epub-reader.md). The flat
[Table view](../usage/table-view.md) is still available with one
row per paragraph (`chapter`, `paragraph`, `text` columns), useful
for searching the book's text with the
[filter bar](../usage/search-and-filter.md) or
[SQL](../usage/sql.md).

### GeoJSON

Read-only. Opens by default in the
[Map view](../usage/view-modes/map.md) with OSM (Open Street Map)
tile background.
The [Table view](../usage/table-view.md) is also available with
one row per Feature; the geometry is serialised as **WKT** in a
`__geometry` column, and every property becomes its own column.

### Archives (zip / tar / tgz)

Read-only. The archive opens as a table listing one row per entry
(`path`, `size_bytes`, `compressed_bytes`, `mtime`, `is_dir`,
`type`). An action bar above the table extracts the selected entry
into a tempfile and opens it as a fresh tab, so any reader Octa
supports works on archive contents. See the
[Archive Viewer](../usage/archive-viewer.md) page for the full
walkthrough.

## Multi-table files

SQLite, DuckDB, and GeoPackage can hold multiple tables. When you
open such a file, Octa shows a **table picker** dialog listing the
available tables with row counts and schemas, so you can pick one
to load. Single-table databases auto-load without the picker. From
the MCP or CLI side, [`list_tables`](../mcp/tools/list_tables.md)
gives you the same enumeration, and every result-bearing MCP tool
accepts a `table` argument to pick one.

## Format conversion

The CLI's [`octa --convert IN OUT`](../cli/convert.md) routes through
the same readers / writers as the GUI, so any read+write pair is a
valid conversion target:

```bash
octa --convert data.csv data.parquet
octa --convert legacy.xlsx tidy.sqlite
octa --convert measurements.dta measurements.json
```

Read-only formats (SAS, RDS, HDF5, NetCDF, EPUB, GeoJSON, archives)
are rejected up-front as conversion targets, so Octa surfaces a
clear error rather than silently writing a malformed file.

## See also

- [`octa --convert`](../cli/convert.md), the CLI for round-tripping
  between any two writable formats.
- [View modes overview](../usage/view-modes/overview.md) covers
  which view Octa picks for each format.
- [Saving files](../usage/saving.md) covers read-only formats and
  diff-based DB writes.
- [Date inference](../reference/date-inference.md) explains how
  string columns in text formats get promoted to typed dates on
  load.
