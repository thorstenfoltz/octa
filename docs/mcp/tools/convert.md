# `convert`

Convert a file from one format to another. Same surface as the CLI's
[`octa --convert`](../../cli/convert.md): both ends resolved by
file extension, both routed through Octa's `FormatRegistry`.

## When to use

- "Convert this Excel to a clean Parquet."
- "Export this SQLite table to CSV."
- "Make a JSON copy of this Stata file."

## Input schema

| Parameter   | Type   | Required? | Default | Description                                                                   |
|-------------|--------|-----------|---------|-------------------------------------------------------------------------------|
| `input`     | string | yes       | —       | Source file path. Format inferred from extension                              |
| `output`    | string | yes       | —       | Destination file path. Format inferred from extension                         |
| `table`     | string | no        | —       | For multi-table input sources, which table to export                          |
| `unlimited` | bool   | no        | `false` | Lift the 5,000,000-row file-loader cap so the entire source file is converted |

## Response shape

```json
{
  "rows_written": <n>,
  "cols_written": <n>,
  "output": "<output_path>"
}
```

## Example calls

### CSV → Parquet

```json
{
  "name": "convert",
  "arguments": {
    "input": "/tmp/sales.csv",
    "output": "/tmp/sales.parquet"
  }
}
```

Response:

```json
{
  "rows_written": 14523,
  "cols_written": 7,
  "output": "/tmp/sales.parquet"
}
```

### Export one SQLite table to JSON

```json
{
  "name": "convert",
  "arguments": {
    "input": "/data/app.sqlite",
    "output": "/tmp/users.json",
    "table": "users"
  }
}
```

### Excel → SQLite

```json
{
  "name": "convert",
  "arguments": {
    "input": "/tmp/quarterly.xlsx",
    "output": "/tmp/quarterly.sqlite"
  }
}
```

## Read-only target rejection

Octa rejects up-front when the output extension maps to a
read-only reader:

| Format                                | Why read-only               |
|---------------------------------------|-----------------------------|
| SAS (`.sas7bdat`)                     | `sas7bdat 0.2` is read-only |
| R datasets (`.rds`, `.rdata`, `.rda`) | `rds2rust` is read-only     |
| HDF5 (`.h5`, `.hdf5`, `.hdf`)         | `hdf5-reader` is read-only  |
| NetCDF (`.nc`)                        | `netcdf3` is read-only      |
| EPUB (`.epub`)                        | Read-only by design         |
| GeoJSON (`.geojson`)                  | Read-only by design         |

Trying to convert *to* one of these errors with:

```json
{ "error": { "code": "invalid_params", "message": "convert failed: format SAS does not support writing; pick a different output extension" }}
```

These same formats are perfectly fine as input: `convert
input.sas7bdat output.csv` is valid.

## Performance and safety

- The input is loaded fully into memory before writing. Streaming
  formats (Parquet, CSV, TSV) honour the initial-load cap during
  the read step (5,000,000 rows by default). For larger
  conversions, pass `unlimited: true`. Parquet files with very many
  row groups fall back to a DuckDB-backed reader automatically.
- `convert` overwrites the output path without prompting, so
  make sure the destination is what you intend.

## Notes

- **Excel writes always produce** `.xlsx` structure regardless of
  the chosen Excel extension, since `rust_xlsxwriter` can't emit
  legacy `.xls` / `.xlsm` / `.xlsb`. Save those as `.xlsx`. `.ods`
  has its own dedicated reader+writer so it round-trips natively.
- **Database outputs** (SQLite, DuckDB, GeoPackage) write a single
  table named after the input's stem. Both `run_sql` (here) and
  `octa --sql` use ephemeral DuckDB sessions, so neither persists
  `CREATE TABLE` / `INSERT` to disk; use `convert` itself, or open
  the target database in the GUI to edit table names directly.

## See also

- [`octa --convert`](../../cli/convert.md) is the CLI equivalent.
- [Supported formats](../../getting-started/supported-formats.md)
  is the full read/write matrix.
