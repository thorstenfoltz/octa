# Saving

Octa supports writing for most formats it reads. Save semantics vary
by format family; this page covers what to expect for each.

## Quick reference

- **File → Save** (Ctrl+S) writes back to the original path in the
  original format.
- **File → Save As…** lets you pick a new path and / or a different
  format. The output format is chosen from the file extension you
  type in the save dialog.
- Closing a tab or quitting Octa with unsaved changes pops a
  *"Save? Don't Save? Cancel?"* confirmation.

The status bar shows a `*` next to the tab name when a tab has
unsaved changes.

## Rounding on save

[Per-column number formats](table-view.md#number-display-separators-and-rounding)
are display-only: the in-memory table keeps full precision. If you set
a rounding format (fixed decimals) on any column and then **Save** or
**Save As**, Octa asks how the file should be written:

- **Save rounded values** writes the rounded numbers shown
  in the table.
- **Save full precision** writes the original, un-rounded
  numbers.
- **Cancel** aborts the save.

Either way the in-memory table stays at full precision, so the choice
only affects the bytes on disk. Tabs without a rounding format save
directly with no prompt.

## Formatting in Excel files

Colour marks, [conditional formatting](conditional-formatting.md) colours,
[frozen columns](table-view.md#freeze-columns) and per-column
[number formats](table-view.md#number-display-separators-and-rounding) are
display-only everywhere else, but `.xlsx` can hold all four. Carrying them
across is **off by default**, since most saves are meant as plain data.

Turn it on under **Settings → Files → Write options → Include formatting in
Excel files**. Independently of that switch, saving a tab that actually
carries any of the four to `.xlsx` asks once:

- **Include formatting** writes the colours, the frozen columns and the
  number formats into the workbook.
- **Plain data** writes the values alone.
- **Do not ask again** stores your answer as the Settings default, so later
  Excel saves go straight through.

Only `.xlsx` is affected. Every other format, `.ods` included, writes plain
data and never asks.

This travels one way. Opening the saved workbook back in Octa reads the values
alone: the reader does not import colours, frozen panes or number formats, so
a round-trip through Octa loses them. The formatting is there for whoever
opens the file in a spreadsheet.

### What Excel cannot say the same way

Most conditional rules cross over as **live** Excel rules, which keep working
when you edit the sheet in Excel. Three kinds cannot, and their colours are
**painted** onto the cells matching today instead. A painted colour is a
snapshot: it stays put when the cell changes in Excel.

- **Ordering comparisons over text** (greater than, less than and so on with
  a non-numeric value). Excel orders text by locale collation, Octa by plain
  character order, so a live rule would colour different cells.
- **Case-sensitive equality or contains rules.** Excel's own equality and
  `SEARCH()` are always case-insensitive, so a live rule would colour more
  cells than Octa does.
- **Every rule after the first painted one.** Octa applies rules
  first-match-wins, but in Excel a live rule always beats a cell's own fill,
  with no notion of order between the two. Painting the remainder is what
  keeps the file agreeing with the screen. Rules before the first painted one
  stay live.

One further difference is accepted rather than engineered around: an equality
rule whose value looks like a number exports as a numeric comparison, while
Octa compares equality as text. A cell holding `42.0` against a rule value of
`42` can therefore colour in Excel but not in Octa.

## Write options

How a file is written is configurable, separately from what is written.
The defaults live under [**Settings → Files → Write
options**](../reference/settings.md) and apply to every save, export and
conversion, including [command-line](../cli/convert.md) conversions that
do not name `--compression` or `--row-group-size` explicitly. [Batch
Convert](batch-convert.md) shows the same controls in its own **Write
options** expander, where they apply to that run only.

Parquet is compressed with **zstd** unless you choose otherwise.
Uncompressed Parquet is only about 1.6x smaller than the same data as
CSV, where zstd reaches roughly 5x, and it costs about 1% more write time
and 3% more read time. The [File internals](file-internals.md) view
reports the codec of any file you open, so you can check what you got.

### Parquet write options

| Option              | Default        | What it does                                                                                         |
|---------------------|----------------|------------------------------------------------------------------------------------------------------|
| Compression         | `zstd`         | Codec for the file. `zstd` is the best size for the cost; `uncompressed` writes fastest and largest. |
| Rows per row group  | writer default | Larger groups scan faster; smaller groups let readers skip more precisely.                           |
| Dictionary encoding | on             | Stores repeated values once. Much smaller files when a column has few distinct values.               |
| Column statistics   | on             | Writes min/max per chunk. Without them a query engine cannot skip row groups and reads everything.   |

### CSV / TSV write options

| Option       | Default          | What it does                                                                                                                |
|--------------|------------------|-----------------------------------------------------------------------------------------------------------------------------|
| Delimiter    | the format's own | Applies to CSV. Left at a comma, each file keeps the delimiter it was opened with; a `.tsv` writes tabs whatever this says. |
| Quoting      | only when needed | Or quote every field, or suppress defensive quoting.                                                                        |
| Line endings | LF               | Switch to CRLF for consumers that require Windows line endings.                                                             |
| Header row   | on               | Turn off to write data only.                                                                                                |

### Excel write options

One switch, **Include formatting in Excel files**, covered in full under
[Formatting in Excel files](#formatting-in-excel-files) above. Off by default.

Apart from the Parquet codec, which is `zstd`, the defaults reproduce
exactly what Octa wrote before these options existed. Formats other than
Parquet, CSV, TSV and `.xlsx` ignore them.

**Save As** uses the Settings defaults: it is the operating system's file
picker, so there is nowhere to put per-save controls. Use Batch Convert
(or the CLI flags below) when you want to override them for one run.

On the command line the same two Parquet knobs are `--compression` and
`--row-group-size`, on both [`--convert`](../cli/convert.md) and
[`--batch-convert`](../cli/batch-convert.md). **Leaving them off uses
these same Settings**, so a conversion in the terminal writes the same
file the app would. A machine with no settings file, such as a container
or a CI runner, falls back to the built-in defaults.

## File-format families

### Text formats (CSV / TSV / JSON / JSONL / XML / TOML / YAML / Markdown / Plain Text)

Straightforward whole-file rewrite. The current table content
replaces the file on disk.

- **CSV** preserves the **original delimiter** it was opened with
  (comma / semicolon / pipe / tab). Octa detects the delimiter on
  open and reuses it on save.
- **TSV** always uses tab.
- **JSON** writes a pretty-printed array of objects keyed by column
  name.
- **JSONL** writes one object per line.
- Quote / escape behaviour for CSV is fixed RFC 4180 on write
  regardless of the
  [Raw text view](view-modes/raw-text.md) display options
  (those only affect viewing; see
  [CSV Quote / Escape](../reference/csv-quote-escape.md)).

### Columnar / data-science (Parquet / Arrow / Avro / ORC)

Whole-file rewrite with the table's current schema. Column types
must round-trip through the format's type system; for unusual types
Octa picks the closest match (most `Decimal` columns lose precision
to `Float64`, for instance).

Parquet's compression and encoding choices use the defaults of the
`arrow`+`parquet` crates.

### Excel (`.xlsx`)

Whole-workbook rewrite via `rust_xlsxwriter`. The current table
becomes the first (and only) worksheet.

Excel **read** supports `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.xlm`
(via `calamine`) and opens **every sheet** of a multi-sheet workbook
(see [Supported Formats](../getting-started/supported-formats.md#excel-multi-sheet-workbooks)).
Excel **write** only emits `.xlsx` structure, since `rust_xlsxwriter`
can't write the older formats, and writes the **active tab's single
sheet**, since there's no multi-sheet write. Save legacy workbooks as
`.xlsx` to round-trip them through Octa.

`.xlsx` is the one format that can carry the table's on-screen formatting;
see [Formatting in Excel files](#formatting-in-excel-files).

### OpenDocument Spreadsheet (`.ods`)

Whole-file rewrite. ODS is handled by Octa's dedicated
[`ods_reader`](https://github.com/thorstenfoltz/octa/blob/master/src/formats/ods_reader.rs)
module: reads go through `calamine`, writes hand-roll a minimal
OpenDocument Spreadsheet 1.2 package (`mimetype` + `META-INF/manifest.xml`

- `content.xml`, zipped). Numbers and booleans are emitted with
typed `office:value`/`office:boolean-value` attributes; everything
else round-trips as strings.

The ODS writer carries less ceremony than `.xlsx`: no styles, no
named ranges, no chart support. If you need those, save as `.xlsx`
instead.

### Statistical (SPSS / Stata)

Whole-file rewrite. SPSS uses `ambers` for write; Stata uses `dta`.
Value labels, formats, and variable labels are preserved when they
round-trip through `DataTable`'s type system; missing-value codes
become `null`.

### DBF (dBase)

Whole-file rewrite. The DBF type system is more constrained than
`DataTable`, so Octa rejects `Binary` columns up front (DBF has no
generic binary type) and widens `Int64` / `UInt64` to wide Numeric
because DBF Integer is i32.

Field names must be ≤ 10 ASCII bytes (DBF spec). Octa will fail the
save with a clear error if a column name is too long; rename in
Octa first.

### Database files (SQLite / DuckDB / GeoPackage)

This is where the save story gets interesting. **DB saves are
diff-based, never overwrite.**

On open, Octa snapshots:

- Every row's `rowid` (SQLite) or synthetic `__octa_row_id`
  (DuckDB / GeoPackage).
- Every row's original cell values.
- The table's column schema.

On save:

1. **DELETE** every original `rowid` missing from the current
   `row_tags` (i.e. rows the user deleted in Octa).
2. **INSERT** every row whose tag is `None` (i.e. rows the user added
   in Octa).
3. **UPDATE** only rows whose content differs from the original
   snapshot. Unchanged rows are skipped, *not* re-written.

All in **one transaction**.

!!! warning "Schema changes are rejected"

    DB save explicitly compares the **current column names** to the
    **original column names**. If they differ (a column was added,
    renamed, deleted, or reordered), the save fails with a clear
    error before touching the file.

    To rename / add / drop a column in a SQLite or DuckDB table,
    open the database in another tool (or run an `ALTER TABLE` via
    Octa's [SQL panel](sql.md) if you load the database
    via DuckDB-attach), then reopen the file in Octa.

    This restriction protects downstream consumers from a column
    suddenly disappearing or being renamed.

The diff-on-save flow means:

- Editing one cell in a 1M-row database table writes one UPDATE, not
  1M.
- New rows get auto-generated `rowid` / sequence values from the
  engine.
- Deleted rows are remembered until save, and undo restores them
  including their original `rowid`.

For GeoPackage specifically, geometry columns round-trip through
WKB. The [Map view](view-modes/map.md) isn't wired to GeoPackage
geometries yet; only GeoJSON triggers the Map view today.

### Read-only formats

| Format                                    | Why                                                                                        |
|-------------------------------------------|--------------------------------------------------------------------------------------------|
| **SAS** (`.sas7bdat`)                     | `sas7bdat 0.2` is read-only.                                                               |
| **R Datasets** (`.rds`, `.rdata`, `.rda`) | `rds2rust` is read-only and Octa only handles the single `data.frame` case anyway.         |
| **HDF5** (`.h5`, `.hdf5`, `.hdf`)         | `hdf5-reader 0.4` is read-only.                                                            |
| **NetCDF v3** (`.nc`)                     | `netcdf3 0.6` is read-only in the upstream crate.                                          |
| **EPUB** (`.epub`)                        | Read-only by design; the [EPUB Reader view](view-modes/epub-reader.md) is a viewer.        |
| **GeoJSON** (`.geojson`)                  | Read-only for now; the [Map view](view-modes/map.md) doesn't currently write back changes. |

To export from a read-only format, use **Save As…** and pick a
writable format (CSV, Parquet, etc.).

## Save As across formats

**File → Save As…** routes through `FormatRegistry`: pick any file
extension that Octa can write and the appropriate writer handles
the conversion. Same as the CLI's
[`octa --convert`](../cli/convert.md).

If you try to Save As into a **read-only target** (`.sas7bdat`,
`.rds`, etc.), the dialog accepts the path but the save fails
loudly with *"format X does not support writing"*.

## Save As respects active filters

When the active tab has a text search or
[column filter](search-and-filter.md#column-filter) applied,
**Save As** writes only the **currently visible** rows. The status
bar confirms the export: *"Exported N filtered rows to {path}
(in-memory table unchanged)"*. The tab's `source_path` is **not**
updated and the modified flag is left alone. Save As under filters
behaves as a one-shot export, not a permanent re-anchor.

Regular **Save** (Ctrl+S) is unaffected by filters: it always writes
the full table back to the source file. This keeps the on-disk file
safe from accidental data loss while filters are active.

## Unsaved-changes guards

Two checkpoints prevent accidental loss:

1. **Closing a tab** with unsaved changes triggers a confirmation
   dialog (Save / Don't Save / Cancel).
2. **Closing the window** (or quitting via menu) with any tab
   having unsaved changes triggers the same dialog, applied to all
   such tabs.

The **Don't Save** path discards every edit including structural
changes. The undo stack is **not** preserved across reloads.

## See also

- [Editing](editing.md) covers what counts as a change.
- [Supported formats](../getting-started/supported-formats.md) is
  the full format matrix.
- [`octa --convert`](../cli/convert.md) drives the same writers
  from the CLI.

### Exporting several tabs as one workbook

**File > Export workbook...** writes any number of open tabs into a single
`.xlsx`, one worksheet per tab. Tick the tabs you want, adjust the sheet names
if you like, and choose where to save.

Sheet names start from the tab labels and are editable, because a tab label can
be long, repeated, or contain characters Excel refuses in a sheet name. Whatever
you leave is corrected before writing: at most 31 characters, no forbidden
punctuation, and duplicates numbered `Report`, `Report_2`. So the export cannot
produce a workbook Excel will not open.

Chart tabs and empty tabs are not offered, since they have no table to write.
The entry has no keyboard shortcut by default; assign one under **Settings >
Shortcuts** if you use it often.

Headless, the same thing is `--to-workbook`:

```bash
octa --to-workbook report.xlsx sales.csv returns.parquet stock.json
```

Sheet names come from the file stems. Agents can do it with the `write_workbook`
tool, which takes an explicit `name` per sheet.
