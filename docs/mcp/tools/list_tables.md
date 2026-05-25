# `list_tables`

List the tables inside a multi-table container (SQLite, DuckDB, or
GeoPackage). Returns each table's name, column schema, and (when
known) row count.

## When to use

- Discovery for any file that *might* be a database. Single-table
  formats (CSV, Parquet) return an empty list, so call
  [`schema`](schema.md) or [`read_table`](read_table.md) directly
  on those.
- Before [`read_table`](read_table.md) on a SQLite file to figure
  out which table to load.
- "What's in this database?" prompts.

## Input schema

| Parameter | Type   | Required? | Description      |
|-----------|--------|-----------|------------------|
| `path`    | string | yes       | Path to the file |

## Response shape

```json
{
  "tables": [
    {
      "name": "<table_name>",
      "columns": [
        { "name": "<column>", "type": "<arrow_type>" },
        …
      ],
      "row_count": <n_or_null>
    },
    …
  ]
}
```

`row_count` is `null` when the reader can't determine it cheaply
without scanning the whole table (rare; SQLite and DuckDB both
report exact counts via `SELECT count(*)`).

For single-table formats, `tables` is an empty array, not an
error.

## Example calls

### List tables in a SQLite database

```json
{
  "name": "list_tables",
  "arguments": { "path": "/data/app.sqlite" }
}
```

Response:

```json
{
  "tables": [
    {
      "name": "users",
      "columns": [
        { "name": "id", "type": "Int64" },
        { "name": "email", "type": "Utf8" },
        { "name": "created_at", "type": "Timestamp(Microsecond, None)" }
      ],
      "row_count": 1247832
    },
    {
      "name": "orders",
      "columns": [
        { "name": "id", "type": "Int64" },
        { "name": "user_id", "type": "Int64" },
        { "name": "amount", "type": "Float64" },
        { "name": "placed_at", "type": "Timestamp(Microsecond, None)" }
      ],
      "row_count": 4891002
    },
    {
      "name": "products",
      "columns": [
        { "name": "id", "type": "Int64" },
        { "name": "sku", "type": "Utf8" },
        { "name": "name", "type": "Utf8" }
      ],
      "row_count": 12408
    }
  ]
}
```

### Single-table file (returns empty)

```json
{
  "name": "list_tables",
  "arguments": { "path": "/tmp/data.parquet" }
}
```

Response:

```json
{ "tables": [] }
```

A model seeing the empty list should fall back to
[`schema`](schema.md) or [`read_table`](read_table.md) without
passing a `table` argument.

## Supported source formats

| Format                                | Behaviour                                                       |
|---------------------------------------|-----------------------------------------------------------------|
| SQLite (`.sqlite`, `.sqlite3`, `.db`) | All user tables; system tables (`sqlite_*`) excluded            |
| DuckDB (`.duckdb`, `.ddb`)            | All user tables; the synthetic `__octa_row_id` column is hidden |
| GeoPackage (`.gpkg`)                  | Spatial + non-spatial tables (geometry blobs surface as Binary) |

Everything else (Parquet, CSV, Excel, etc.) returns an empty list.

## See also

- [`schema`](schema.md) and [`read_table`](read_table.md) accept
  the `table` value from a `list_tables` response.
- [Supported formats](../../getting-started/supported-formats.md)
  lists which formats are single-table vs multi-table.
