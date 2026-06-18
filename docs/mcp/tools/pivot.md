# `pivot`

Reshape a table between **long and wide** form using DuckDB `PIVOT` / `UNPIVOT`.
The same engine that backs the GUI Pivot / Unpivot dialog. Read-only analytics
(stays available under `--mcp-read-only`).

## When to use

- Spread a category column into one column per value (long -> wide).
- Melt many measure columns back into key/value pairs (wide -> long).

## Input schema

| Parameter   | Type     | Required?    | Default        | Description                                                          |
|-------------|----------|--------------|----------------|----------------------------------------------------------------------|
| `path`      | string   | yes*         | (no default)   | Path to the file (omit when `open_tab` is set)                       |
| `open_tab`  | string   | no           | (no default)   | Operate on an open GUI tab (`@active` or a tab name)                 |
| `table`     | string   | no           | (no default)   | Specific table for multi-table sources                               |
| `mode`      | string   | no           | `pivot`        | `pivot` (long -> wide) or `unpivot` (wide -> long)                   |
| `on`        | string   | pivot only   | (no default)   | Pivot: column whose distinct values become new columns               |
| `value`     | string   | pivot only   | (no default)   | Pivot: column aggregated under each new column                       |
| `agg`       | string   | no           | `sum`          | Pivot: `sum` / `count` / `avg` / `min` / `max`                       |
| `group`     | string[] | no           | (inferred)     | Pivot: identity columns kept as rows (empty = DuckDB infers them)    |
| `columns`   | string[] | unpivot only | (no default)   | Unpivot: the columns to melt (at least two)                          |
| `name_col`  | string   | no           | `name`         | Unpivot: name of the generated key column                            |
| `value_col` | string   | no           | `value`        | Unpivot: name of the generated value column                          |
| `limit`     | integer  | no           | server default | Cap response rows (`0` = unlimited)                                  |
| `unlimited` | bool     | no           | `false`        | Lift the 5,000,000-row file-loader cap so the reshape sees every row |

## Response shape

The reshaped table in the standard row-returning shape:

```json
{
  "schema": [ { "name": "region", "type": "Utf8" }, … ],
  "rows": [ [ … ], … ],
  "row_count": 12,
  "truncated": false,
  "total_rows_available": 12,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "pivot",
  "arguments": {
    "path": "/tmp/sales.parquet",
    "mode": "pivot",
    "on": "quarter",
    "value": "amount",
    "agg": "sum",
    "group": ["region"]
  }
}
```

## See also

- [`run_sql`](run_sql.md): custom reshaping with `PIVOT` / `UNPIVOT` or `CASE`.
- [`profile`](profile.md): per-column statistics.
