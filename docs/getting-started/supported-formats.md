# Supported Formats

Octa reads ~35 file formats out of the box. Most are also writable.
Unknown extensions fall back to the plain-text reader so you can
always open *something*.

## At-a-glance matrix

| Format                        | Extensions                                         | Read | Write |
|-------------------------------|----------------------------------------------------|:----:|:-----:|
| **Parquet**                   | `.parquet`                                         |  ✅   |   ✅   |
| **CSV / TSV**                 | `.csv`, `.tsv`                                     |  ✅   |   ✅   |
| **JSON**                      | `.json`                                            |  ✅   |  ✅ †  |
| **JSON Lines**                | `.jsonl`, `.ndjson`                                |  ✅   |  ✅ †  |
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
| **XML**                       | `.xml`                                             |  ✅   |  ✅ †  |
| **TOML**                      | `.toml`                                            |  ✅   |  ✅ †  |
| **YAML**                      | `.yaml`, `.yml`                                    |  ✅   |  ✅ †  |
| **Jupyter notebook**          | `.ipynb`                                           |  ✅   |   ✅   |
| **Markdown**                  | `.md`, `.markdown`, `.mdown`, `.mkd`               |  ✅   |   ✅   |
| **HTML**                      | `.html`, `.htm`                                    |  ✅   |   ❌   |
| **SQL dump**                  | `.sql` (opt-in, see below)                         |  ✅   |   ❌   |
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

† **Nested documents flatten.** See
[Round-trip fidelity](#round-trip-fidelity) below.

## Caveats and limitations by format

### Round-trip fidelity

Reading and writing a format does not by itself mean opening a file and
saving it gives you the file you started with.

Octa's working model is a flat table of typed cells. Whatever a format
carries beyond that is kept only where the reader and writer were built
as a matching pair. Where they were, it is deliberate and documented:

- **Excel** keeps cell formatting and formulas, both as opt-in write
  options. See [Write options](../usage/saving.md#write-options).
- **Jupyter notebooks** keep cell outputs when you save an edited
  notebook.
- **Markdown, source code and plain text** open as one row per line and
  are written back line for line, so editing prose or code in Octa is
  lossless.
- **SQLite, DuckDB and GeoPackage** are edited in place with a diff, so
  everything you did not touch (other tables, indexes, views) stays as it
  was.

Where they were not, the loss is structural rather than a defect:

- **JSON, JSON Lines, XML, TOML and YAML** are nested document formats,
  and a table is not nested. Reading flattens nested objects into dotted
  column names (`address.city`); writing emits a flat array of records,
  or for XML a generic `<data><row>` document. Open a deeply nested
  configuration file, save it, and you get a table of it, not the
  original document. Use Octa to *inspect* those files, and a text editor
  to edit their structure.
- **Excel** always writes `.xlsx` structure, whatever the extension you
  read (see the footnote above).
- **SPSS** variable and value labels are read past, not carried: a `.sav`
  written by Octa has the data and the column names, not the study
  metadata the original carried.

The rule of thumb: if the thing you care about is the rows and columns,
Octa round-trips it. If the thing you care about is how the file was
arranged around them, open it, look, and save it somewhere else.

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

Missing values in character columns now read correctly. Earlier versions
showed `NA_character_` as the literal text `NA`, which was impossible to
tell apart from a genuine `"NA"` value in your data; such cells are now
empty, like missing values in every other column type.

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

### HTML

Read-only. **Every `<table>` on the page becomes a table**, the way every
sheet of a workbook does: they open in their own tabs, subject to the same
auto-open cap and multi-select picker. A table with a `<caption>` is named
after it; the rest are `Table 1`, `Table 2`, and so on.

The parser is the same one browsers use, so the tag soup of a real page
parses like a page rather than failing like a strict XML document. Three
things worth knowing:

- **`rowspan` and `colspan` are expanded into repeated cells.** A cell
  spanning three columns becomes that value three times, because a grid
  with holes cannot be sorted or filtered.
- **A leading row of `<th>` becomes the header.** Without one the columns
  are numbered.
- **Nested tables are listed too**, and their text also stays in the cell
  that holds them. A table whose cells hold nothing but another table is a
  layout wrapper and is skipped, so an old-fashioned page does not turn
  into a single enormous cell.

Values arrive as text and are then promoted by the usual load passes, so a
column of numbers is a number column and
[dates are inferred](../reference/date-inference.md) as they are anywhere
else.

There is no separate download step: **File > Open URL** fetches the page
and hands it straight to this reader, so a Wikipedia article opens as its
tables. To see the markup itself instead, use **View > Reopen as > Text**.

### SQL dump

Read-only, and **off by default**. A `.sql` file opens as text, which is what
it usually is. To read one as its tables instead, pick the reader by name:

- **File > Open as > SQL dump** for a file that is not open yet, or
- **View > Reopen as > SQL dump** for the one you are already looking at.

Opening a `.sql` that turns out to hold `CREATE TABLE` and `INSERT INTO`
statements says so in the status bar and points at the second of those, so the
reader is one click away rather than something you have to already know about.

`mysqldump`, `pg_dump` and `sqlite3 .dump` output all work. Rather than parse
SQL, Octa scrubs the dialect-only spellings off each statement and replays the
file into a scratch database in memory, then reads that the way it reads a
`.sqlite` file. So a dump with six tables offers the same table picker any
database file does, and the table you pick opens in the current tab if it is
empty and in a new one otherwise, leaving the text you were reading open.

What that handles, because real dumps are full of it: MySQL's conditional
`/*!40101 ... */` comments, `AUTO_INCREMENT`, per-column `COLLATE`,
`CHARACTER SET` and `COMMENT`, `enum(...)` columns, the `KEY` and `UNIQUE KEY`
index clauses, `ENGINE=InnoDB DEFAULT CHARSET=...` table options,
`DEFAULT current_timestamp()`, and backslash escapes inside strings. On the
Postgres side: schema qualifiers (`public.orders` becomes `orders`),
`timestamp with time zone`, the `\restrict` line newer `pg_dump` versions
start with, and **`COPY ... FROM stdin` blocks**, which are how a default
`pg_dump` writes its rows.

**A statement that will not replay is skipped, not fatal.** Every dump is full
of statements no other engine can run (`SET`, `LOCK TABLES`,
`ALTER TABLE ... OWNER TO`), and refusing the file over them would help nobody.
Only failures of statements that carry schema or data are counted, and when
there are any, a dismissible banner says how many and quotes the first, so a
half-imported table is never silent.

Two limits worth knowing:

- **512 MB.** The file is replayed into memory, so a bigger dump is refused
  with a sentence rather than by filling the machine. Load a large dump into a
  real database and connect to that instead.
- **Read-only.** Octa is showing you a snapshot of the dump, not editing it.
  Save As is how you keep a table.

Asking for a reader by name is a menu, so this is the desktop app only: the
command line and the MCP server read a `.sql` as the text it is.

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

## Regional conventions in delimited files

Files written on a German, French or Scandinavian machine differ from
the Anglo-American default in three ways, all handled without a setting:

- **Semicolon separators.** The delimiter is detected from the first
  lines of the file (`,`, `;`, `|` and tab are all recognised), so a
  `;`-separated export opens correctly.
- **European numbers.** `1.234,56` and `3,14` are read as numbers rather
  than text, decided per column. Undecidable columns such as a whole
  column of `1,234` raise a small dialog instead of being guessed at.
  See [European number formats](../usage/editing.md#european-number-formats).
- **Dotted dates.** `31.12.2024` is a recognised date layout; see
  [Date Inference](../reference/date-inference.md).

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

Every tab is labelled `workbook.xlsx - Sheet name`, so three sheets of one
file are three tabs you can tell apart rather than three tabs called
`workbook.xlsx`. The same goes for a table you pick out of a SQLite or DuckDB
file. Renaming the tab (right-click it) still overrides the label.

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
