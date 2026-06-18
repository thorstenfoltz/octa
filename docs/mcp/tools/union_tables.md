# `union_tables`

Stack two or more tables vertically into one result, taking the union of
their columns. Columns missing from a source are filled with nulls; mixed
numeric types widen to a common number type, otherwise the column falls
back to text.

## When to use

- Combining several exports of the same shape (monthly files, per-region
  dumps) into one table.
- Merging an open GUI tab with a file on disk.

## Input schema

| Parameter   | Type      | Required? | Default        | Description                                                                 |
|-------------|-----------|-----------|----------------|-----------------------------------------------------------------------------|
| `sources`   | object[]  | yes       | (no default)   | Two or more sources. Each has `path` (file) **or** `open_tab` (`@active` / tab name), plus optional `table` for multi-table sources |
| `drop`      | string[]  | no        | `[]`           | Column names to exclude from the output (unknown names ignored)             |
| `cast`      | object[]  | no        | `[]`           | Per-column target-type overrides (`{ "column": NAME, "type": ARROW_TYPE }`) |
| `limit`     | integer   | no        | server default | Max rows to return. `0` = unlimited                                         |
| `unlimited` | bool      | no        | `false`        | Lift the 5,000,000-row file-loader cap so every source row is read          |

## Response shape

Returns the combined table:

```json
{
  "schema": [ { "name": "id", "type": "Int64" }, … ],
  "rows": [ [ … ], … ],
  "row_count": 120,
  "truncated": false,
  "total_rows_available": 120,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "union_tables",
  "arguments": {
    "sources": [
      { "path": "/data/jan.csv" },
      { "path": "/data/feb.csv" }
    ]
  }
}
```

## See also

- [`join_tables`](join_tables.md): match rows side-by-side on a key instead
  of stacking them.
- [`run_sql`](run_sql.md): `UNION ALL` for custom column handling.
