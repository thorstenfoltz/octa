# `partition_table`

Split a table into one file per distinct value of a column, written into a
directory. This is a **write** tool: it is removed when the server runs
with `--mcp-read-only`.

## When to use

- Sharding a dataset by category, region, date, etc.
- Producing per-group files for downstream tools.

## Input schema

| Parameter  | Type   | Required? | Default            | Description                                                                                   |
|------------|--------|-----------|--------------------|-----------------------------------------------------------------------------------------------|
| `path`     | string | no\*      | (no default)       | Path to the source file (omit when `open_tab` is set)                                         |
| `open_tab` | string | no        | (no default)       | Operate on an open GUI tab (`@active` or tab name)                                            |
| `table`    | string | no        | (no default)       | Specific table for multi-table sources                                                        |
| `column`   | string | yes       | (no default)       | Column whose distinct values become the partitions                                            |
| `out_dir`  | string | yes       | (no default)       | Output directory (created if absent)                                                          |
| `format`   | string | no        | source's extension | Output extension without the dot (`csv`, `parquet`, …). Required for an open tab with no file |

\* `path` or `open_tab` is required.

## Response shape

```json
{
  "files": [
    { "value": "North", "path": "/out/North.csv", "rows": 40 },
    { "value": "South", "path": "/out/South.csv", "rows": 35 }
  ],
  "count": 2
}
```

## Example call

```json
{
  "name": "partition_table",
  "arguments": {
    "path": "/data/sales.csv",
    "column": "region",
    "out_dir": "/out/by_region",
    "format": "parquet"
  }
}
```

## See also

- [`run_sql`](run_sql.md) with `write_to`: custom per-group queries.
