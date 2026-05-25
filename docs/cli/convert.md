# `octa --convert`

Convert a file from one format to another, going through Octa's
shared `FormatRegistry`, the same reader and writer the GUI uses.
Format inference is **extension-driven**: `.csv` reads as CSV,
`.parquet` writes as Parquet, etc.

## Synopsis

```bash
octa --convert IN OUT
```

Both `IN` and `OUT` are file paths. Octa picks readers/writers based
on the extensions. The `-f / --format` flag has **no effect** here:
`--convert`'s output format is locked to the output extension.

## Examples

```bash
# CSV → Parquet
octa --convert sales.csv sales.parquet

# Excel → SQLite
octa --convert workbook.xlsx tidy.sqlite

# JSON → Arrow IPC
octa --convert data.json data.arrow

# Stata → CSV
octa --convert survey.dta survey.csv

# JSON Lines → DuckDB
octa --convert events.jsonl events.duckdb
```

On success, Octa writes a summary to stderr:

```text
wrote 14523 rows × 7 columns to sales.parquet
```

(stderr so it doesn't contaminate the data going to stdout, even
though `--convert` writes to a file rather than stdout.)

## Read-only target rejection

A handful of formats are read-only: Octa knows how to parse them
but can't write them back:

- SAS (`.sas7bdat`)
- R datasets (`.rds`, `.rdata`, `.rda`)
- HDF5 (`.h5`, `.hdf5`, `.hdf`)
- NetCDF v3 (`.nc`)
- EPUB (`.epub`)
- GeoJSON (`.geojson`)

If you try to use one as the **output** of `--convert`, Octa rejects
the request with a clear error before touching the file:

```bash
$ octa --convert data.parquet data.sas7bdat
error: format SAS does not support writing; pick a different output extension
```

These formats work fine as **input**: `octa --convert input.sas7bdat
output.csv` is perfectly valid.

## What conversions are safe

The general rule: anything Octa reads to the same `DataTable`
representation, Octa writes consistently. Some round-trips lose
fidelity at the format boundary:

| Conversion                 | Notes                                                                                                                                                             |
|----------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Parquet ↔ Arrow IPC        | Lossless. Same Arrow type system underneath.                                                                                                                      |
| CSV → Parquet              | Type inference applies on read (numeric detection, date inference). The Parquet output is properly typed.                                                         |
| Parquet → CSV              | Round-trip safe for plain types; `Decimal` columns serialise to text.                                                                                             |
| Anything → SQLite / DuckDB | Schema preserved; one table named after the file's stem.                                                                                                          |
| SQLite / DuckDB → Anything | The selected table's data is exported.                                                                                                                            |
| Anything → Excel           | Single worksheet, no formatting. Excel's per-cell character limit (32,767) is enforced by `rust_xlsxwriter`; cells longer than that fail the write with an error. |
| Anything → JSON            | Pretty-printed array of objects. Binary cells become hex strings.                                                                                                 |

## When to use it

- **One-shot reformat**, preferred over opening in the GUI and
  Save-As when you don't need to inspect the data.
- **Pipelines**: `octa --convert in.csv stage1.parquet` is part of
  CI / batch jobs.
- **Type coercion**: round-trip CSV → Parquet → CSV to apply
  Octa's type inference and normalise the date columns.

For non-trivial transformations, `--sql` followed by `--convert` is
the usual pattern (run a SQL query, save the result):

```bash
# Filter rows then convert
octa --sql in.csv -q 'SELECT * FROM data WHERE region = "EU"' -f csv > eu.csv
octa --convert eu.csv eu.parquet
```

## Notes

- **Stdin / stdout aren't supported.** Both paths must be real files.
  Octa needs the extension to pick the format.
- **CSV delimiter is preserved on input** (Octa detects the
  delimiter on open), and is `,` by default on output.
- **Multi-table sources** (SQLite, DuckDB with > 1 table) export
  the **first** table only. To export a specific table, open the
  file in the GUI, pick the table, and use **File → Save As**.
- **Memory**: `--convert` loads the input table fully into memory
  before writing. For files larger than RAM, slice with
  `octa --sql ... LIMIT N` first.

## See also

- [Supported formats](../getting-started/supported-formats.md) for
  the full read/write matrix.
- [`octa --sql`](sql.md) for filtering before conversion.
- [MCP `convert` tool](../mcp/tools/convert.md) is the same
  conversion surface via MCP.
