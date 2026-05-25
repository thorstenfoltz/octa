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
(via `calamine`). Excel **write** only emits `.xlsx` structure,
since `rust_xlsxwriter` can't write the older formats. Save legacy
workbooks as `.xlsx` to round-trip them through Octa.

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
