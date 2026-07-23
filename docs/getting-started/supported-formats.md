# Supported Formats

Octa reads ~25 file formats out of the box. Most are also writable.
Unknown extensions fall back to the plain-text reader so you can
always open *something*.

## At-a-glance matrix

| Format                        | Extensions                                         | Read | Write |
|-------------------------------|----------------------------------------------------|:----:|:-----:|
| **Parquet**                   | `.parquet`                                         |  ✅   |   ✅   |
| **CSV / TSV**                 | `.csv`, `.tsv`                                     |  ✅   |   ✅   |
| **JSON**                      | `.json`                                            |  ✅   |   ✅   |
| **JSON Lines**                | `.jsonl`, `.ndjson`                                |  ✅   |   ✅   |
| **Excel**                     | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.xlm`          |  ✅   |  ✅ *  |
| **ODS**                       | `.ods`                                             |  ✅   |   ✅   |
| **Arrow IPC / Feather**       | `.arrow`, `.feather`                               |  ✅   |   ✅   |
| **Avro**                      | `.avro`                                            |  ✅   |   ✅   |
| **ORC**                       | `.orc`                                             |  ✅   |   ✅   |
| **HDF5**                      | `.h5`, `.hdf5`, `.hdf`                             |  ✅   |   ❌   |
| **NetCDF v3**                 | `.nc`                                              |  ✅   |   ❌   |
| **NumPy**                     | `.npy`, `.npz`                                     |  ✅   |   ❌   |
| **SQLite**                    | `.sqlite`, `.sqlite3`, `.db`                       |  ✅   | ✅ **  |
| **DuckDB**                    | `.duckdb`, `.ddb`                                  |  ✅   | ✅ **  |
| **GeoPackage**                | `.gpkg`                                            |  ✅   | ✅ **  |
| **SAS**                       | `.sas7bdat`                                        |  ✅   |   ❌   |
| **SPSS**                      | `.sav`, `.zsav`                                    |  ✅   |   ✅   |
| **Stata**                     | `.dta`                                             |  ✅   |   ✅   |
| **R Datasets**                | `.rds`, `.rdata`, `.rda`                           |  ✅   |   ❌   |
| **DBF / dBase**               | `.dbf`                                             |  ✅   |   ✅   |
| **XML**                       | `.xml`                                             |  ✅   |   ✅   |
| **TOML**                      | `.toml`                                            |  ✅   |   ✅   |
| **YAML**                      | `.yaml`, `.yml`                                    |  ✅   |   ✅   |
| **Jupyter notebook**          | `.ipynb`                                           |  ✅   |   ✅   |
| **Markdown**                  | `.md`, `.markdown`, `.mdown`, `.mkd`               |  ✅   |   ✅   |
| **EPUB**                      | `.epub`                                            |  ✅   |   ❌   |
| **GeoJSON**                   | `.geojson`                                         |  ✅   |   ❌   |
| **Shapefile**                 | `.shp` (+ sibling `.dbf`, `.shx`)                  |  ✅   |   ❌   |
| **Delta Lake**                | table *directory* (`_delta_log/`)                  |  ✅   |   ❌   |
| **Apache Iceberg**            | table *directory* (`metadata/`)                    |  ✅   |   ❌   |
| **MessagePack**               | `.msgpack`, `.mpk`                                 |  ✅   |   ❌   |
| **BSON**                      | `.bson`                                            |  ✅   |   ❌   |
| **Archive (zip / tar / tgz)** | `.zip`, `.tar`, `.tgz`                             |  ✅   |   ❌   |
| **Fixed-width (FWF)**         | `.fwf`, `.prn`                                     |  ✅   |   ❌   |
| **Source code / config**      | `.py`, `.rs`, `.go`, `.ts`, `.js`, ... (see below) |  ✅   |   ✅   |
| **Plain text**                | anything else                                      |  ✅   |   ✅   |

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
(more than 32,767, which is common with Spark or streaming ingest
pipelines) exceed the native arrow-parquet reader's limit
(`Row group ordinal 32768 exceeds i16 max value`). Octa reads
those files through a DuckDB-backed reader automatically, with the same
schema and types and no user action required.

Files produced by **pandas** (`DataFrame.to_parquet`) embed the row
index as an extra column on disk (`__index_level_0__` by default,
or whatever you passed to `set_index`). Octa strips those columns
on read so the table view shows only the real data columns. Both
the Arrow schema metadata's `index_columns` entries and the
default `__index_level_0__` name are honoured, including on files
written by older pandas releases that didn't emit the metadata
block.

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

### NumPy

Read-only. A `.npy` file holds a single array: a 1-D array opens
as one `value` column, a 2-D array as one column per column index
(`col_0`, `col_1`, ...), and higher dimensions flatten their
trailing axes into columns. A `.npz` file is a zip of named arrays
(what `numpy.savez` writes), so it opens as a multi-table source,
one table per array, picked from the table dialog. Structured /
record arrays are not supported.

### MessagePack and BSON

Read-only. Both are binary cousins of JSON, so Octa decodes them
and flattens them the same way as JSON: nested objects become
dotted columns (`address.city`) and a top-level array of objects
becomes one row per object. A MessagePack file holds a single
value; a `.bson` file may hold several documents back-to-back (the
shape `mongodump` writes), each becoming a row. Dates, ObjectIds
and other BSON-specific values render in MongoDB's relaxed extended
JSON form.

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

### Shapefile

Read-only. A shapefile is a set of sibling files: open the `.shp`
and Octa pulls geometry from it and attributes from the matching
`.dbf` (the `.shx` index is read too). It opens just like GeoJSON,
in the [Map view](../usage/view-modes/map.md), with a `__geometry`
WKT column followed by one column per attribute field. Keep the
companion files next to the `.shp`. Writing is not supported.

### Delta Lake and Apache Iceberg

Read-only, and what you open is a **directory**, not a single file:
a Delta or Iceberg table is a folder of Parquet data files plus a
transaction log (`_delta_log/`) or metadata layer (`metadata/`) that
records which files form the current snapshot. Use
**File -> Open table folder...** and pick the table directory; Octa
detects whether it is Delta or Iceberg and reads the current
snapshot through DuckDB's `delta_scan` / `iceberg_scan`.

Two things to know:

- The DuckDB `delta` / `iceberg` **extensions install on first use,
  which needs network access**. After that they are cached and work
  offline.
- The directory must be **complete**: the log/metadata plus every
  Parquet file it references. A single `.parquet` lifted out of such
  a table is just a fragment, open it with the
  [Parquet reader](#streaming-readers-large-files-ok) instead.

### Archives (zip / tar / tgz)

Read-only. The archive opens as a table listing one row per entry
(`path`, `size_bytes`, `compressed_bytes`, `mtime`, `is_dir`,
`type`). An action bar above the table extracts the selected entry
into a tempfile and opens it as a fresh tab, so any reader Octa
supports works on archive contents. See the
[Archive Viewer](../usage/archive-viewer.md) page for the full
walkthrough.

### Fixed-width (FWF)

Read-only, best-effort. Fixed-width files have no delimiter: each
field sits in a fixed range of character columns, padded with
spaces. Octa infers the column boundaries by sampling the leading
lines and finding the character positions that are blank in every
line (the gaps between fields), and treats the first line as the
header (blank header cells become `col_1`, `col_2`, ...). All
columns are read as text. Detection works best on cleanly aligned
exports (typical mainframe / spreadsheet `.prn` output); a column
whose values run together with its neighbour cannot be split.
Claims `.fwf` and `.prn` only (not `.txt`, which stays plain text).

### Source code and config files

Octa opens common source-code and configuration files as plain text
(one row per line) and syntax-highlights them in the
[Raw view](../usage/view-modes/overview.md). Because they are
registered formats, they appear in the open dialog's **All Supported**
filter rather than only opening via the catch-all fallback. Recognised
extensions include:

- **Python** `.py`, `.pyw`, `.pyi`
- **Rust** `.rs`
- **Shell** `.sh`, `.bash`, `.zsh`, `.fish`
- **C / C++** `.c`, `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp`, `.hxx`
- **Go** `.go`
- **JS / TS / Web** `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, `.tsx`,
  `.html`, `.htm`, `.css`, `.scss`, `.sass`
- **JVM** `.java`, `.kt`, `.kts`, `.scala`, `.groovy`
- **Scripting** `.rb`, `.php`, `.pl`, `.lua`, `.swift`
- **Data science** `.r`, `.jl`
- **Terraform / HCL** `.tf`, `.tfvars`, `.hcl`
- **Container files** `Dockerfile`, `Dockerfile.*` (e.g. `Dockerfile.dev`),
  `Containerfile`, `Containerfile.*` - these have no extension but Octa
  recognises them by name, opens them with syntax highlighting, and shows them
  in the sidebar file browser.
- **Misc** `.tex`, `.dart`, `.ex`, `.exs`, and the plain-text /
  config set (`.txt`, `.log`, `.ini`, `.cfg`, `.conf`, `.env`, ...)

Any other unknown extension still opens through the plain-text reader,
so you can always open *something*.

### Text file encodings

Text, source-code, and Markdown files do not have to be UTF-8. Octa
detects the encoding automatically: it honours a byte-order mark (BOM),
takes the UTF-8 fast path when the bytes are valid UTF-8, and otherwise
falls back to character-set detection. Files saved as **Windows-1252 /
Latin-1** or **UTF-16** (common on non-English Windows, and from Excel's
"Unicode text" export) open correctly instead of failing or showing
garbled characters. The detected text is decoded to UTF-8 in memory; your
file on disk is untouched.

CSV and TSV use their own streaming decoder and can additionally be
re-decoded through the [malformed-file repair](#repairing-malformed-csv-tsv-files)
prompt.

## Wrong or missing file extensions

Octa does not rely on the extension alone. When a file's extension is
missing, wrong, or unrecognised, it looks at the **content** to pick a
reader:

- **Magic bytes** identify binary formats regardless of name, a
  Parquet file called `export.bin`, a SQLite database with no
  extension, a ZIP-based archive, and so on.
- **Structure probes** recognise text formats: a `.txt` that is
  actually JSON, or a delimited file whose extension doesn't match.

This works in two places. When opening a file, Octa consults the
content sniffer before falling back to plain text. And if the reader
chosen from the extension *errors* (for example a `.csv` that is really
Parquet), Octa retries with the sniffed reader instead of just showing
a parse error. The upshot: renamed and mislabelled files usually just
open as the right thing.

## Repairing malformed CSV / TSV files

CSV and TSV files in the wild are often slightly broken: the wrong text
encoding, a stray byte-order mark (BOM) at the start, control
characters, a delimiter that disagrees with the extension (a `.csv`
that is really tab-separated), or ragged rows with uneven column
counts. Octa can offer to clean these up on open.

This is **off by default**. Turn on **Offer repair on malformed files**
in [**Settings → File-Specific**](../reference/settings.md#file-specific).
With it on, when a CSV/TSV reads but looks malformed, a prompt appears
that lists what was detected and shows a preview of the repaired result.
You choose:

- **Repair and open** re-decodes the text, re-detects the delimiter,
  and strips stray markers.
- **Open without repair** loads the file as-is.
- **Cancel** backs out.

When **ragged rows** are detected (some rows have more fields than the
header), the prompt also offers **Keep extra values (add columns)**. With
it ticked, repair **widens** the table so every extra field keeps its own
column (the overflow columns are named `column_4`, `column_5`, ...) instead
of being dropped. Rows that are too short are padded with empty cells. This
is on by default for ragged files, because dropping data is rarely the fix
you want; untick it to fall back to trimming each row to the header width.

The repair only changes what Octa loads into memory, **your file on
disk is never modified**. It applies to CSV/TSV only. See
[CSV quote / escape](../reference/csv-quote-escape.md) for the related
quoting and delimiter rules.

## Multi-table files

SQLite, DuckDB, and GeoPackage can hold multiple tables. When you
open such a file, Octa shows a **table picker** dialog listing the
available tables with row counts and schemas, so you can pick one
to load. Single-table databases auto-load without the picker. From
the MCP or CLI side, [`list_tables`](../mcp/tools/list_tables.md)
gives you the same enumeration, and every result-bearing MCP tool
accepts a `table` argument to pick one.

### Excel multi-sheet workbooks

Excel workbooks behave differently from databases: Octa treats each
worksheet as a table and opens several at once, each in its
own tab.

- If the workbook has up to N sheets, all of them open
  automatically. `N` is the Excel sheets to auto-open (default 5),
  can be changed in
  [**Settings → Performance**](../reference/settings.md#performance).
- If it has more than N, a sheet picker appears listing every
  sheet with the first `N` pre-checked. Tick the ones you want
  (**Select all** / **Select none** help) and click **Open**. You
  can pick any number of sheets, including all of them.

The first row of each sheet is used as the header row, the same as the
single-sheet behaviour.

## Compressed files

Gzip (`.gz`) and Zstandard (`.zst`) inputs decompress transparently:
`data.csv.gz` opens as a normal CSV, in the GUI, the CLI, and the MCP
tools alike. The inner format comes from the middle extension. Saving a
compressed file recompresses it back with the same codec. A
decompression size cap (Settings > Files > Max decompressed size,
default 4 GB) guards against decompression bombs.

## Datasets (folder of parts)

A directory can be a table too. **File > Open table folder...** (or
right-click a directory in the folder sidebar and pick **Open as
dataset...**) opens:

- **Delta Lake** directories (marked by `_delta_log/`) and **Apache
  Iceberg** directories (marked by `metadata/`), read through DuckDB's
  extensions (installed over the network on first use, then cached).
- Any other directory holding data parts: Parquet, CSV/TSV, or JSON
  Lines files (scanned up to 8 levels deep). The majority family is
  read as one table and a banner lists any skipped files.

## Format conversion

The CLI's [`octa --convert IN OUT`](../cli/convert.md) routes through
the same readers / writers as the GUI, so any read+write pair is a
valid conversion target:

```bash
octa --convert data.csv data.parquet
octa --convert legacy.xlsx tidy.sqlite
octa --convert measurements.dta measurements.json
```

Read-only formats (SAS, RDS, HDF5, NetCDF, NumPy, MessagePack, BSON,
EPUB, GeoJSON, Shapefile, Delta Lake, Iceberg, archives) are rejected up-front as conversion targets, so Octa surfaces a
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
