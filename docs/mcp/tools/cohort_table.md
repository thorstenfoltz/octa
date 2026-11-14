# `cohort_table`

Build a cohort table: each person (`who`) belongs to the period of their
first row (`when`), and each cell says how many of a cohort were active `k`
periods later, as a share, a count or a column's sum. The same engine as the
GUI's [Cohorts](../../usage/cohorts.md) and `octa --cohort`.

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name         | Type   | Meaning                                                             |
|--------------|--------|---------------------------------------------------------------------|
| `path`       | string | The file. May be a cloud URL.                                       |
| `open_tab`   | string | Use an open GUI tab instead (name, or `@active`).                   |
| `table`      | string | Sheet or table name for multi-table sources.                        |
| `who`        | string | The column naming a person or customer.                             |
| `when`       | string | The date or timestamp column (ISO text works).                      |
| `period`     | string | `day`, `week` (Monday start), `month` (default), `quarter`, `year`. |
| `show`       | string | `share` (default, percent of the cohort), `count` or `sum`.         |
| `sum_column` | string | The number column for `show: sum`.                                  |
| `limit`      | number | Rows in the response. Pass `0` for unlimited.                       |

## Response shape

For `visits.csv` with `who: "who"`, `when: "when"`:

```json
{
  "skipped": 0,
  "table": {
    "schema": [
      { "name": "cohort", "type": "Utf8" },
      { "name": "size", "type": "Int64" },
      { "name": "month_0", "type": "Float64" },
      { "name": "month_1", "type": "Float64" }
    ],
    "rows": [["2024-01", 2, 100.0, 50.0], ["2024-02", 1, 100.0, null]],
    "row_count": 2,
    "truncated": false,
    "total_rows_available": 2
  }
}
```

A `null` cell is a period the data does not reach yet. `skipped` counts the
rows with no `who` or no readable date.

## See also

- In the app: [Cohorts](../../usage/cohorts.md)
- Command line: [`--cohort`](../../cli/cohort.md)
