# `partition_table`

Split a table into files: one per distinct value of a column, and/or parts
of at most `max_rows` rows or `max_bytes` bytes (files then get a part
number). Pass at least one of `column`, `max_rows`, `max_bytes`. This is a **write** tool: it is removed when the
server runs with `--mcp-read-only`.

## When to use

- Sharding a dataset by category, region, date, etc.
- Producing per-group files for downstream tools.

## Input schema

| Parameter   | Type   | Required? | Default            | Description                                                                                        |
|-------------|--------|-----------|--------------------|----------------------------------------------------------------------------------------------------|
| `path`      | string | no\*      | (no default)       | Path to the source file (omit when `open_tab` is set)                                              |
| `open_tab`  | string | no        | (no default)       | Operate on an open GUI tab (`@active` or tab name)                                                 |
| `table`     | string | no        | (no default)       | Specific table for multi-table sources                                                             |
| `column`    | string | no        | (no default)       | Column whose distinct values become the partitions. Optional when `max_rows` or `max_bytes` is set |
| `max_rows`  | number | no        | (no default)       | At most this many rows per file                                                                    |
| `max_bytes` | number | no        | (no default)       | At most this many bytes per file (they land a little under it)                                     |
| `unlimited` | bool   | no        | `false`            | Lift the streaming load cap so every source row is split                                           |
| `out_dir`   | string | yes       | (no default)       | Output directory (created if absent)                                                               |
| `format`    | string | no        | source's extension | Output extension without the dot (`csv`, `parquet`, …). Required for an open tab with no file      |

\* `path` or `open_tab` is required.

## Response shape

```json
{
  "files": [
    { "value": "North", "path": "/out/North.csv", "rows": 40 },
    { "value": "South", "path": "/out/South.csv", "rows": 35 }
  ],
  "count": 2,
  "partial": false
}
```

`value` is `null` when the split is not by a column. `partial` is `true` when
the source was cut at the load cap; pass `unlimited: true` to split every row.

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

Split into parts of 100,000 rows:

```json
{
  "name": "partition_table",
  "arguments": {
    "path": "/data/sales.csv",
    "out_dir": "/out/parts",
    "max_rows": 100000
  }
}
```

## See also

- [`run_sql`](run_sql.md) with `write_to`: custom per-group queries.
- In the app: [Partition by Column](../../usage/partition-by-column.md)
- Command line: [`--partition-by`](../../cli/partition.md)
