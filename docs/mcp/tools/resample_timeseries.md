# `resample_timeseries`

Group rows into **time buckets**: one output row per interval of a timestamp
column. The same engine that backs the GUI Time series dialog and CLI
`--resample`. Read-only analytics (stays available under `--mcp-read-only`).

## When to use

- Turn per-transaction rows into daily / weekly / monthly totals.
- Produce one series per category with `group_by`, ready to chart.

## Input schema

| Parameter    | Type     | Required? | Default        | Description                                                           |
|--------------|----------|-----------|----------------|-----------------------------------------------------------------------|
| `path`       | string   | yes*      | (no default)   | Path to the file (omit when `open_tab` is set)                        |
| `open_tab`   | string   | no        | (no default)   | Operate on an open GUI tab (`@active` or a tab name)                  |
| `table`      | string   | no        | (no default)   | Specific table for multi-table sources                                |
| `time_col`   | string   | yes       | (no default)   | The timestamp column to bucket                                        |
| `value_cols` | string[] | yes       | (no default)   | Columns aggregated within each bucket                                 |
| `interval`   | string   | no        | `day`          | `minute`/`hour`/`day`/`week`/`month`/`quarter`/`year`                 |
| `agg`        | string   | no        | `sum`          | `sum`/`mean`/`min`/`max`/`count`/`first`/`last`                       |
| `group_by`   | string[] | no        | `[]`           | One series per combination of these columns                           |
| `limit`      | integer  | no        | server default | Cap response rows (`0` = unlimited)                                   |
| `unlimited`  | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so the resample sees every row |

The bucket lands in a column named `bucket`, or `bucket_2` if the source
already has a column called `bucket`.

## Response shape

The resampled table in the standard row-returning shape:

```json
{
  "schema": [ { "name": "bucket", "type": "Utf8" }, … ],
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
  "name": "resample_timeseries",
  "arguments": {
    "path": "/tmp/sales.csv",
    "time_col": "day",
    "value_cols": ["amount"],
    "interval": "month",
    "agg": "sum",
    "group_by": ["region"]
  }
}
```

## Notes

The timestamp column is cast with `TRY_CAST`, so one unparseable value lands
in a null bucket instead of failing the whole call.

## See also

- [`rolling_window`](rolling_window.md): moving averages over the previous N rows.
- [`pivot`](pivot.md): reshape between long and wide form.
- [`run_sql`](run_sql.md): custom `date_trunc` grouping when the parameters do not fit.
