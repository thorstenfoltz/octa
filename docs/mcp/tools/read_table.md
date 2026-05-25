# `read_table`

Read a tabular data file and return both the column schema and the
rows. This is the workhorse tool: most prompts that involve "look at
this file" end up calling `read_table`.

## When to use

Use `read_table` when you want to give Claude the actual data, not
just metadata. For peek-only operations, [`schema`](schema.md) and
[`count_rows`](count_rows.md) are cheaper.

For multi-table sources (SQLite, DuckDB, GeoPackage), call
[`list_tables`](list_tables.md) first to discover available tables,
then pass `table` here.

## Input schema

| Parameter   | Type   | Required? | Default               | Description                                                                                |
|-------------|--------|-----------|-----------------------|--------------------------------------------------------------------------------------------|
| `path`      | string | yes       | (no default)          | Absolute or working-directory-relative path to the file                                    |
| `limit`     | int    | no        | server default (1000) | Maximum rows to return in the response. `0` means unlimited                                |
| `table`     | string | no        | (no default)          | Specific table to read for multi-table sources                                             |
| `unlimited` | bool   | no        | `false`               | Lift the 5,000,000-row file-loader cap so every row is read from disk. Use with `limit: 0` |

## Response shape

```json
{
  "schema": [
    { "name": "<column>", "type": "<arrow_type>" },
    …
  ],
  "rows": [
    [<v>, <v>, …],
    …
  ],
  "row_count": <n>,
  "truncated": <bool>,
  "total_rows_available": <n>,
  "cell_truncated": <bool>
}
```

The `rows` are an array of arrays (positional, matching the order of
`schema`). Cells are JSON-typed: integers and floats keep their
native JSON types; strings, dates, datetimes are JSON strings; binary
cells are hex-encoded strings; nulls are `null`.

## Example calls

### Basic read

```json
{
  "method": "tools/call",
  "params": {
    "name": "read_table",
    "arguments": {
      "path": "/tmp/sales.parquet"
    }
  }
}
```

Response (abbreviated):

```json
{
  "schema": [
    { "name": "region", "type": "Utf8" },
    { "name": "amount", "type": "Float64" }
  ],
  "rows": [
    ["EU", 1245.50],
    ["US", 89.00],
    ["APAC", 2100.00]
  ],
  "row_count": 1000,
  "truncated": true,
  "total_rows_available": 47832,
  "cell_truncated": false
}
```

### Read a specific table from a SQLite database

```json
{
  "name": "read_table",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "users",
    "limit": 100
  }
}
```

### Unlimited (every row)

There are two caps to lift, in two different places:

- `limit` controls how many rows the *response* JSON carries
  (default 1000). `limit: 0` removes that ceiling.
- `unlimited: true` controls how many rows the *file loader* reads
  off disk (default 5,000,000 for streaming formats). Without it,
  the response can never contain more rows than the file loader
  actually loaded — `limit: 0` alone tops out at 5 Million.

Combine both to truly read every row, after checking the file isn't
multi-GB:

```json
{
  "name": "read_table",
  "arguments": {
    "path": "/tmp/small.csv",
    "limit": 0,
    "unlimited": true
  }
}
```

The defaults exist exactly because dumping every row of every file
through stdio scales badly. Default to staying inside the caps; opt
out only when the user has a real need.

## Behaviour for specific formats

| Format                                 | Notes                                                                                                                                                                                         |
|----------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Parquet, CSV, TSV                      | Streaming readers: load the first 5 M rows (the server's initial-load cap, lifted by `unlimited: true`), then truncate to `limit`. Parquet files with > 32,767 row groups fall back to DuckDB |
| SQLite / DuckDB / GeoPackage           | Multi-table: pass `table` to pick. Default reads the first table                                                                                                                              |
| Excel / SPSS / Stata / SAS / RDS / DBF | Full file load; `limit` truncates after the read                                                                                                                                              |
| HDF5 / NetCDF                          | Same; full load + truncate                                                                                                                                                                    |
| EPUB                                   | Returns the paragraph table: `chapter`, `paragraph`, `text`                                                                                                                                   |
| GeoJSON                                | Returns one row per feature with WKT in `__geometry`                                                                                                                                          |

## See also

- [Limits & truncation](../limits-and-truncation.md) explains what
  `truncated` and `cell_truncated` mean in practice.
- [`schema`](schema.md) is a schema-only call when rows aren't
  needed.
- [`run_sql`](run_sql.md) is for filtered or aggregated results.
