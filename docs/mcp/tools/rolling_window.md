# `rolling_window`

Add a **rolling aggregate** column over the previous N rows. The same engine
that backs the GUI Time series dialog and CLI `--rolling`. Read-only analytics
(stays available under `--mcp-read-only`).

## When to use

- A moving average that smooths noisy per-row values.
- A running total, minimum or maximum in a defined order.

## Input schema

| Parameter      | Type     | Required? | Default        | Description                                                         |
|----------------|----------|-----------|----------------|---------------------------------------------------------------------|
| `path`         | string   | yes*      | (no default)   | Path to the file (omit when `open_tab` is set)                      |
| `open_tab`     | string   | no        | (no default)   | Operate on an open GUI tab (`@active` or a tab name)                |
| `table`        | string   | no        | (no default)   | Specific table for multi-table sources                              |
| `order_col`    | string   | yes       | (no default)   | The column that orders the frame                                    |
| `value_col`    | string   | yes       | (no default)   | The column aggregated over the frame                                |
| `window`       | integer  | yes       | (no default)   | Rows in the frame, **including** the current one. At least 1        |
| `agg`          | string   | no        | `mean`         | `sum`/`mean`/`min`/`max`/`count`/`first`/`last`                     |
| `partition_by` | string[] | no        | `[]`           | Restarts the frame per combination of these columns                 |
| `limit`        | integer  | no        | server default | Cap response rows (`0` = unlimited)                                 |
| `unlimited`    | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so the window sees every row |

`order_col` is required by design: a rolling aggregate over unordered rows is
meaningless, so there is no way to omit it.

## Response shape

Every source column plus one named `<value_col>_rolling_<window>`, in the
standard row-returning shape:

```json
{
  "schema": [ …, { "name": "amount_rolling_7", "type": "Utf8" } ],
  "rows": [ [ … ], … ],
  "row_count": 365,
  "truncated": false,
  "total_rows_available": 365,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "rolling_window",
  "arguments": {
    "path": "/tmp/sales.csv",
    "order_col": "day",
    "value_col": "amount",
    "window": 7,
    "agg": "mean",
    "partition_by": ["region"]
  }
}
```

## See also

- [`resample_timeseries`](resample_timeseries.md): group into time buckets instead.
- [`run_sql`](run_sql.md): custom window frames (`RANGE`, `FOLLOWING`, …).
