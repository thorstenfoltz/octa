# `write_table`

Write model-supplied rows to a file in any writable format. It is the
inverse of [`read_table`](read_table.md): you provide the schema and the
rows inline, and Octa serialises them through its `FormatRegistry` based
on the output extension.

## When to use

- "Save these rows as a CSV / Parquet / Excel file."
- "Create a small lookup table from this data."
- "Append these new records to the existing export."

For editing an *existing* file's cells or rows, use
[`edit_table`](edit_table.md). For persisting a SQL result to a database,
use [`run_sql`](run_sql.md) with `write_to`.

## Input schema

| Parameter   | Type   | Required? | Default      | Description                                                                           |
|-------------|--------|-----------|--------------|---------------------------------------------------------------------------------------|
| `path`      | string | yes       | (no default) | Destination file path. Format inferred from extension                                 |
| `columns`   | array  | yes       | (no default) | Column definitions, in order. Each is `{ "name": string, "type"?: string }`           |
| `rows`      | array  | no        | `[]`         | Array-of-arrays. Each inner array is one row, lined up positionally with `columns`    |
| `mode`      | string | no        | `create`     | `create` (error if file exists), `overwrite` (replace), or `append` (add to existing) |
| `unlimited` | bool   | no        | `false`      | Lift the 5,000,000-row file-loader cap when reading the existing file for `append`    |

`type` is an Arrow type name (e.g. `Int64`, `Float64`, `Boolean`,
`Date32`, `Timestamp(Microsecond, None)`, `Utf8`) and defaults to `Utf8`.
Cells are coerced to the column type: integers into a `Float64` column
become floats; a string into a `Binary` column is hex-decoded when it is
valid hex; JSON arrays/objects are stored verbatim as nested text.

The `rows` shape is identical to what `read_table` returns, so a read
result round-trips straight back in.

## Response shape

```json
{
  "rows_written": <n>,
  "cols_written": <n>,
  "output": "<path>",
  "mode": "<mode>"
}
```

## Example calls

### Create a CSV from inline rows

```json
{
  "name": "write_table",
  "arguments": {
    "path": "/tmp/people.csv",
    "columns": [
      { "name": "id", "type": "Int64" },
      { "name": "name", "type": "Utf8" }
    ],
    "rows": [[1, "alice"], [2, "bob"]],
    "mode": "create"
  }
}
```

Response:

```json
{ "rows_written": 2, "cols_written": 2, "output": "/tmp/people.csv", "mode": "create" }
```

### Append rows to an existing file

```json
{
  "name": "write_table",
  "arguments": {
    "path": "/tmp/people.csv",
    "columns": [
      { "name": "id", "type": "Int64" },
      { "name": "name", "type": "Utf8" }
    ],
    "rows": [[3, "carol"]],
    "mode": "append"
  }
}
```

In `append` mode the file must already exist and its column **names** must
match `columns`; otherwise the call errors with an
`append column mismatch` message.

## Modes and safety

- `create` refuses to clobber an existing file (returns an error telling
  you to use `overwrite` or `append`).
- `overwrite` replaces the whole file without prompting.
- `append` reads the existing file fully (pass `unlimited: true` for very
  large files), validates the schema, and rewrites it with the extra
  rows. For row-oriented formats this is a full rewrite, not an in-place
  append.

## Read-only and database targets

- Read-only output formats (SAS, RDS, HDF5, NetCDF, EPUB, GeoJSON) are
  rejected up front, the same as [`convert`](convert.md).
- Database files (`.sqlite`, `.duckdb`) are **not** valid `write_table`
  targets, because their writers need a table loaded from the database
  (diff-based save). Use [`edit_table`](edit_table.md) to edit an existing
  database table, or [`run_sql`](run_sql.md) `write_to` to persist a query
  result.

## See also

- [`read_table`](read_table.md) is the inverse read.
- [`edit_table`](edit_table.md) edits an existing file in place.
- [Supported formats](../../getting-started/supported-formats.md) is the
  full read/write matrix.
