# `data_drift`

Compare two versions of the same dataset and report how it moved:
columns added or removed, and per shared column the change in null rate,
distinct count and, for numeric columns, minimum, maximum and mean.
Columns with few distinct values also report which category values
appeared and vanished.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name                        | Type   | Meaning                                                               |
|-----------------------------|--------|-----------------------------------------------------------------------|
| `path_a` / `path_b`         | string | The earlier and later versions. May be cloud URLs.                    |
| `open_tab_a` / `open_tab_b` | string | Use an open GUI tab instead (name, or `@active`).                     |
| `table_a` / `table_b`       | string | Sheet or table name for multi-table sources.                          |
| `category_cap`              | number | Skip category comparison above this many distinct values. Default 50. |
| `fail_on`                   | string | `metric:change` gates, e.g. `null_rate:0.05,rows:0.1`.                |
| `unlimited`                 | bool   | Lift the streaming row cap for this call.                             |

## Response

```json
{
  "rows_before": 1000,
  "rows_after": 1000,
  "added_columns": ["region"],
  "removed_columns": [],
  "failed": true,
  "drift": [
    {
      "column": "amount",
      "metric": "null_rate",
      "before": 0.0,
      "after": 0.5,
      "before_text": "0",
      "after_text": "0.5",
      "change": null,
      "breached": true
    }
  ]
}
```

`failed` is true when any gate was breached, so it can be used directly
as a pass/fail. A baseline of zero that moved at all counts as an
unbounded change, which is why `change` can be null on a breached row.

This measures distributions, not rows. Use
[`diff_tables`](diff_tables.md) when the question is which rows changed.

See [Data Drift](../../usage/data-drift.md).
