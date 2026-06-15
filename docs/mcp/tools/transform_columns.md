# `transform_columns`

**Rename, cast, or drop columns** of a tabular file and write the result back.
This is the column-level edit that [`edit_table`](edit_table.md) deliberately
does not do (that one only changes cells/rows).

This is a **write** tool, so it is removed under `--mcp-read-only` (alongside
`write_table`, `edit_table`, `convert`, and `anonymize`).

## When to use

- Tidy up a schema before a load: drop junk columns, rename, fix types.
- Re-type a string column to `Int64` / `Float64` / `Date32` and convert its
  cells in one pass.

## Input schema

| Parameter     | Type     | Required? | Default          | Description                                                        |
|---------------|----------|-----------|------------------|--------------------------------------------------------------------|
| `path`        | string   | yes       | (no default)     | Path to the source file                                            |
| `drop`        | string[] | no        | `[]`             | Column names to drop (applied first)                              |
| `rename`      | object[] | no        | `[]`             | `{ "from": NAME, "to": NAME }` pairs (applied after drops)        |
| `cast`        | object[] | no        | `[]`             | `{ "column": NAME, "type": ARROW_TYPE }` (applied last)           |
| `output_path` | string   | no        | overwrite `path` | Where to write the result; format follows its extension          |
| `unlimited`   | bool     | no        | `false`          | Lift the 5,000,000-row file-loader cap so every row is rewritten  |

Operations apply in a fixed order: **drop**, then **rename**, then **cast** (so
cast/rename refer to the post-drop column set, and cast uses the new names).
`type` is an Arrow type name, e.g. `Int64` / `Float64` / `Utf8` / `Boolean` /
`Date32`; values that cannot be converted stay as text. Database files
(SQLite / DuckDB / GeoPackage) are not valid sources or targets.

## Response shape

```json
{
  "rows_written": 1000,
  "cols_written": 4,
  "columns": [ { "name": "id", "type": "Int64" }, … ],
  "output": "/tmp/cleaned.parquet"
}
```

## Example call

```json
{
  "name": "transform_columns",
  "arguments": {
    "path": "/tmp/raw.csv",
    "output_path": "/tmp/cleaned.parquet",
    "drop": ["notes"],
    "rename": [ { "from": "amt", "to": "amount" } ],
    "cast": [ { "column": "amount", "type": "Float64" } ]
  }
}
```

## See also

- [`edit_table`](edit_table.md): change cells / insert / delete rows in place.
- [`anonymize`](anonymize.md): mask / scramble column values.
