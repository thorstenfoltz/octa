# Supported Formats

Octa reads over 40 file formats out of the box, and about half are also
writable.
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
| **MATLAB**                    | `.mat`                                             |  ✅   |   ❌   |
| **NetCDF v3**                 | `.nc`                                              |  ✅   |   ❌   |
| **NumPy**                     | `.npy`, `.npz`                                     |  ✅   |   ❌   |
| **SQLite**                    | `.sqlite`, `.sqlite3`, `.db`                       |  ✅   | ✅ **  |
| **DuckDB**                    | `.duckdb`, `.ddb`                                  |  ✅   | ✅ **  |
| **GeoPackage**                | `.gpkg`                                            |  ✅   | ✅ **  |
| **Microsoft Access**          | `.mdb`, `.accdb`                                   |  ✅   |   ❌   |
| **SAS**                       | `.sas7bdat`                                        |  ✅   |   ❌   |
| **SAS transport** (XPORT)     | `.xpt`, `.xport`                                   |  ✅   |   ✅   |
| **QlikView data** (QVD)       | `.qvd`                                             |  ✅   |   ❌   |
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
| **PDF** (tables)              | `.pdf`                                             |  ✅   |   ❌   |
| **SQL dump**                  | `.sql` (opt-in, see below)                         |  ✅   |   ❌   |
| **EPUB**                      | `.epub`                                            |  ✅   |   ❌   |
| **GeoJSON**                   | `.geojson`                                         |  ✅   |   ❌   |
| **Shapefile**                 | `.shp` (+ sibling `.dbf`, `.shx`)                  |  ✅   |   ❌   |
| **GPX**                       | `.gpx`                                             |  ✅   |   ❌   |
| **KML / KMZ**                 | `.kml`, `.kmz`                                     |  ✅   |   ❌   |
| **Delta Lake**                | table *directory* (`_delta_log/`)                  |  ✅   |   ❌   |
| **Apache Iceberg**            | table *directory* (`metadata/`)                    |  ✅   |   ❌   |
| **Lance**                     | dataset *directory* (`.lance`, `_versions/`)       |  ✅   |   ✅   |
| **MessagePack**               | `.msgpack`, `.mpk`                                 |  ✅   |   ❌   |
| **BSON**                      | `.bson`                                            |  ✅   |   ❌   |
| **Archive (zip / tar / tgz)** | `.zip`, `.tar`, `.tgz`                             |  ✅   |   ❌   |
| **Fixed-width (FWF)**         | `.fwf`, `.prn`                                     |  ✅   |   ❌   |
| **Log files**                 | `.log`, `syslog`, `access.log.1`                   |  ✅   |   ❌   |
| **Email / Mailbox**           | `.eml`, `.mbox`                                    |  ✅   |   ❌   |
| **Outlook**                   | `.msg`, `.oft`, `.pst`, `.olm`                     |  ✅   |   ❌   |
| **iCalendar / vCard**         | `.ics`, `.vcs`, `.vcf`                             |  ✅   |   ❌   |
| **Source code / config**      | `.py`, `.rs`, `.go`, `.ts`, `.js`, ... (see below) |  ✅   |   ✅   |
| **Plain text**                | anything else                                      |  ✅   |   ✅   |

\* **Excel write** always produces `.xlsx` structure, because the
writer uses `rust_xlsxwriter` which doesn't emit legacy `.xls` /
`.xlsm` / `.xlsb`. Save those as `.xlsx` to round-trip them through
Octa.

DuckDB's lists, vectors, structs and maps show as text, such as `[0.5, 1.5]`
or `{'x': 1}`, also in SQL results. You can edit them there, and a value that
no longer fits its column stops the save. Saved to another format, they are
written as that text.

Decimal columns keep every digit, and sort and sum as numbers. Parquet and Arrow
files get a real decimal column, and the other formats hold the digits as they
are. A value that does not fit the column, such as text typed into a cell or a
number too long for it, stops a save to Parquet or Arrow with an error, and digits
past the column's scale round. Numbers with a fraction in a text file are read
as exact decimals too, and sums add them without rounding: see
[Exact numbers](../usage/exact-numbers.md).

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
- **Numbers in YAML and TOML** are floating point by those formats' own
  rules (TOML's specification makes a float a 64-bit double, YAML's float
  is an approximation). A number past about 15 digits rounds on reading;
  such a number Octa saves is written as quoted text so no digit is lost.
  Quote it in the file to keep it exact. See
  [Exact numbers](../usage/exact-numbers.md#known-limits).
- **Excel** always writes `.xlsx` structure, whatever the extension you
  read (see the footnote above).
- **Times and durations** keep their type where the format has one
  (Parquet, Arrow, DuckDB, Excel, ODS, Avro and TOML clock times) and are
  written as their shown text elsewhere. See
  [Times and durations](../usage/times-and-durations.md#saving).
- **SPSS** variable and value labels are read past, not carried: a `.sav`
  written by Octa has the data and the column names, not the study
  metadata the original carried.

The rule of thumb: if the thing you care about is the rows and columns,
Octa round-trips it. If the thing you care about is how the file was
arranged around them, open it, look, and save it somewhere else.

### Streaming readers (large files OK)

Parquet, CSV, and TSV all stream. Octa loads the first
`AppSettings.initial_load_rows` (default 2,000,000) rows and
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

### SAS transport files

A `.xpt` (or `.xport`) file is the exchange format SAS writes
for other programs, and the one regulators such as the FDA ask for.
Octa reads and writes version 5 and version 8 files.

- A variable with a SAS date, datetime or time format opens as a date,
  a date and time, or a time. A time that runs past a day opens as a
  duration.
- SAS has one number type. A column opens as whole numbers when every
  value is whole, else as decimals.
- Missing numbers, including the special ones (`.A` to `.Z`, `._`),
  open as empty cells.
- A file holding several datasets offers them in the table picker.
- Variable labels are not read.

A version 5 file does not store its row count. Trailing rows holding
nothing but blanks are taken as padding, as SAS itself does, so a real
last row that is entirely blank text is not shown.

Saving writes one dataset, named after the file.

- Version 5, the one regulators ask for, is written whenever the table
  fits it: column names of at most 8 characters and text cells of at
  most 200 bytes. A table that does not fit is written as version 8
  (names up to 32 characters).
- A column name must be a SAS name: letters, digits and `_`, not
  starting with a digit. Octa refuses any other name and tells you
  which one, it does not rename the column for you.
- Dates, dates with times and times are written with the SAS formats
  `DATE9.`, `DATETIME19.` and `TIME8.`. True and false become 1 and 0.
  Empty cells become SAS missing values.
- A SAS number carries about 16 digits. A longer exact decimal is
  rounded to that.
- A file that holds several datasets is not overwritten, because that
  would delete the others. Save under another name.

### Microsoft Access databases

Read-only. Octa opens `.mdb` and `.accdb` files from Access 97 to
Access 2019 without Access or a driver installed, on every platform.

- The table picker lists the tables. System tables and hidden tables
  are left out, as are the columns Access hides.
- A Date/Time column opens as a date when no value has a time of day,
  else as a date and time.
- Currency and Decimal columns keep every digit.
- Yes/No opens as true and false.
- Attachments and multi-value fields open empty. Queries, forms and
  reports are not shown.
- A database protected with a password opens too. The app asks for the
  password, as it does for a protected zip, and can remember it in your
  keychain. The command line and the MCP server read it from the
  environment variable `OCTA_ACCESS_PASSWORD`. See
  [Password-protected files](../usage/protected-files.md).

### QlikView data files

Read-only. A `.qvd` file is one table saved by QlikView or Qlik Sense.

- Dates, timestamps, times and intervals open typed, from the field's
  number format or its `$date` / `$timestamp` tag.
- A field of numbers opens as whole numbers or decimals.
- Qlik can store a number together with the text it is shown as. Where
  that text says more than the number (`007`, `EUR 12.50`), the column
  opens as text, so you see what Qlik shows.

The whole file is read into memory.

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

### MATLAB

Read-only. Every variable in a `.mat` file is a table, offered in the table
picker; a plain open (and the command line without `--table`) shows the first
one that reads. Both layouts are read: version 5 (what MATLAB's `-v6` and
`-v7` write, compressed or not) and version 7.3, which is HDF5 inside.

| Variable                               | Becomes                                                    |
|----------------------------------------|------------------------------------------------------------|
| A number or true/false vector          | One column named after the variable                        |
| A matrix                               | Columns `c1`, `c2`, ...                                    |
| Text (a char array)                    | One row per text row                                       |
| A cell of single values                | One column (or `c1`, `c2`, ... for a two-dimensional cell) |
| A struct whose fields are equally long | One column per field, the usual way to keep a table        |
| A struct array of single values        | One row per element, one column per field                  |

MATLAB keeps every number as a double, so a column holding only whole numbers
opens as whole numbers, and `NaN` opens as an empty cell. MATLAB `table`,
`string` and `datetime` objects, class instances, sparse and complex
matrices, function handles and arrays of more than two dimensions are listed
but do not open; the message says which it is. Version 4 files are not read.

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

### PDF

Read-only. Octa finds the tables in the text of a PDF (invoices, bank
statements, reports) and lists them in the table picker as `Page 2,
table 1` and so on; pick one and it opens as a normal tab. A table is
text laid out in two or more aligned columns, and the lines just above
it (titles, headings) are left out.

**Scanned pages are pictures, not text.** Octa does no OCR, so a table
on a scanned page cannot be read, and Octa says so rather than showing
nothing. See [Tables from PDFs](../usage/pdf-tables.md).

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
  Save as is how you keep a table.

Asking for a reader by name is a menu, so this is the desktop app only: the
command line and the MCP server read a `.sql` as the text it is.

### EPUB

Read-only. Octa converts each chapter's XHTML to Markdown at load
time and renders chapter-by-chapter in the
[EPUB reader view](../usage/view-modes/epub-reader.md). The flat
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

### GPX

Read-only. A GPX file holds GPS points from a watch, a phone or a bike
computer. Octa shows one row per point:

| Column       | Holds                                               |
|--------------|-----------------------------------------------------|
| `kind`       | `waypoint`, `track` or `route`                      |
| `track`      | The name of the track or route the point belongs to |
| `segment`    | The track segment, counted from 1 (tracks only)     |
| `name`       | The point's own name, when it has one               |
| `lat`, `lon` | The position in degrees                             |
| `elevation`  | Metres above sea level                              |
| `time`       | When the point was recorded, in UTC                 |

It opens in the Table view, and the
[Map view](../usage/view-modes/map.md) plots the points, since the table
has `lat` and `lon` columns. A track is drawn as its points, not as a line.
Extensions such as heart rate or cadence are not read.

### KML and KMZ

Read-only. KML is the format of Google Earth and Google My Maps; a `.kmz`
is the same file zipped, and Octa reads the `doc.kml` inside it (or the
first `.kml` it finds). One row per Placemark:

| Column        | Holds                                                    |
|---------------|----------------------------------------------------------|
| `name`        | The placemark's name                                     |
| `description` | Its description, as written (it may contain HTML)        |
| `folder`      | The folders it sits in, joined with ` / `                |
| `kind`        | `Point`, `LineString`, `Polygon` or `MultiGeometry`      |
| `lat`, `lon`  | The position of its first point, in degrees              |
| `altitude`    | The altitude of its first point, when the file gives one |

It opens in the [Map view](../usage/view-modes/map.md), where lines and
polygons are drawn whole, holes included. A placemark without a place keeps
its row, with empty coordinates. Styles, `gx:Track` and network links are
not read. A `.kmz` is unpacked up to the decompression limit in Settings.

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

### Lance

Read and written. A Lance dataset is a **directory** holding `_versions/` and
`data/`, usually named `something.lance`, and it is often used for machine
learning data with vector columns.

- **To open one**, use **File -> Open table folder...**, or right-click the
  folder in the sidebar and pick **Open as dataset...**. On the command line
  and in MCP tools, pass the folder as you would a file:
  `octa --head vectors.lance`.
- **To create one**, use **File -> Save As...** with a name ending in `.lance`,
  or `octa --convert data.csv vectors.lance`.

What to know:

- **The first use downloads DuckDB's `lance` extension**, about 100 MB, which
  takes about 300 MB on disk. After that it is cached and works offline. Octa
  reads and saves these datasets in the background, so the window stays usable
  meanwhile. The extension exists for Linux, Windows on x86-64 and macOS on
  Apple silicon.
- **Every save adds a new version** holding the whole table. The version before
  stays in the folder, so a save that fails changes nothing, but the folder grows
  with each save. This is why auto-save skips Lance tabs. Octa cannot list these
  versions yet, and it rebuilds no index built on the dataset.
- **Column types are kept**, including vectors, lists and structs. These show as
  text such as `[0.5, 1.5]`, which you can edit. A value that no longer fits its
  column stops the save with an error, and the dataset stays as it was.
- **Durations are written as text**, because Lance cannot store DuckDB's interval
  type. Lance also refuses a column name holding a dot.
- **Octa will not write over** a plain file or a folder holding other files.
- **A dataset is a folder, not a file**, so it cannot go into an encrypted zip, be
  uploaded to cloud storage in one piece, go to stdout or be split by a size limit.
  A single `.lance` file inside `data/` is one fragment, not a table: opening
  one in the app opens the dataset it belongs to, and the command line asks for
  the folder.

### Archives (zip / tar / tgz)

Read-only. The archive opens as a table listing one row per entry
(`path`, `size_bytes`, `compressed_bytes`, `mtime`, `is_dir`,
`type`). An action bar above the table extracts the selected entry
into a tempfile and opens it as a fresh tab, so any reader Octa
supports works on archive contents. See the
[Archive Viewer](../usage/archive-viewer.md) page for the full
walkthrough.

### Mail, calendars and contacts

Each file becomes one table per kind of item it holds (Mail, Calendar,
Contacts, Tasks, Notes) plus an Attachments table whose `content` cells hold
the files. Bodies are plain text (an RTF-only Outlook body stays empty),
times are shown as written, recurring events stay one row with their rule as
text. `.ost`/`.nst` offline caches, `.pab` and `.nk2` are not read. See
[Mail, calendar and contact files](../usage/mail-and-calendar-files.md).

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
  config set (`.txt`, `.ini`, `.cfg`, `.conf`, `.env`, ...). A `.log` that
  is not a log in a known format opens as plain text too; see
  [Log files](../usage/log-files.md).

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
encoding, control characters, a delimiter that disagrees with the
extension (a `.csv` that is really tab-separated), or ragged rows with
uneven column counts. Octa can offer to clean these up on open. A UTF-8
byte order mark at the start is not one of them: spreadsheet programs
want it, Octa writes it on request, and it is read without a prompt.

This is **off by default**. Turn on **Offer repair on malformed files**
in [**Settings → Opening files**](../reference/settings.md#opening-files).
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

SQLite, DuckDB, GeoPackage and Microsoft Access can hold multiple tables,
and so can a SAS transport file with several datasets. When you
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
decompression size cap (Settings > Performance > Decompression size cap,
default 4 GB, or `--max-decompressed` on the command line) guards against
decompression bombs.

## Datasets (folder of parts)

A directory can be a table too. **File > Open table folder...** (or
right-click a directory in the folder sidebar and pick **Open as
dataset...**) opens:

- **Delta Lake** directories (marked by `_delta_log/`), **Apache
  Iceberg** directories (marked by `metadata/`) and **Lance** datasets
  (marked by `_versions/`), read through DuckDB's extensions (installed
  over the network on first use, then cached).
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

Read-only formats (SAS, Microsoft Access, QVD, RDS, HDF5, MATLAB, NetCDF, NumPy, MessagePack, BSON,
EPUB, PDF, GeoJSON, Shapefile, GPX, KML, Delta Lake, Iceberg, archives) are rejected up-front as conversion targets, so Octa surfaces a
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
