# `seasonality`

Split one column over time into its trend, its repeating pattern and the rest
(the classical additive decomposition), to tell a spike that is unusual from
one that comes every season. The season is read from the date spacing (24
hourly, 7 daily, 52 weekly, 12 monthly, 4 quarterly). The same split as the
GUI's [Seasonality to table](../../usage/forecast.md#seasonality-to-table) and
`octa --seasonality`.

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name       | Type   | Meaning                                                   |
|------------|--------|-----------------------------------------------------------|
| `path`     | string | The file. May be a cloud URL.                             |
| `open_tab` | string | Use an open GUI tab instead (name, or `@active`).         |
| `table`    | string | Sheet or table name for multi-table sources.              |
| `x`        | string | Time column: dates, date-times or numbers, evenly spaced. |
| `y`        | string | Column of numbers to split.                               |
| `season`   | number | Season length in points. Default: read from the spacing.  |
| `limit`    | number | At most this many points in the answer; `0` for all.      |
| `limit`    | number | At most this many points in the answer; `0` for all.      |

## Response shape

For `monthly_sales.csv` with `x: "month"`, `y: "sales"` (two of the 48 rows):

```json
{
  "season": 12,
  "strength": 0.996,
  "decomposition": {
    "schema": [
      { "name": "x", "type": "Date32" },
      { "name": "value", "type": "Float64" },
      { "name": "trend", "type": "Float64" },
      { "name": "seasonal", "type": "Float64" },
      { "name": "rest", "type": "Float64" }
    ],
    "rows": [
      ["2022-01-01", 98.4, null, -16.472, null],
      ["2022-07-01", 142.0, 123.25, 17.972, 0.778]
    ],
    "row_count": 48,
    "truncated": false,
    "total_rows_available": 48
  }
}
```

`strength` is the share of the movement around the trend that the pattern
explains, 0 to 1. `trend` and `rest` are null for the first and last half
season. Yearly data and plain numbers name no season, so pass `season` for
those. Fewer than two full seasons, or uneven x values, are refused with the
reason; bucket uneven data first with `resample_timeseries`.

## See also

- Command line: [`--seasonality`](../../cli/seasonality.md)
- In the app: [Seasonality to table](../../usage/forecast.md#seasonality-to-table)
