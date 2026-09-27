# `forecast`

Forecast one column over time with Holt-Winters: the level, the trend and a
season read from the date spacing (24 hourly, 7 daily, 52 weekly, 12 monthly,
4 quarterly). The same model as the GUI's
[Trend and forecast](../../usage/forecast.md) and `octa --forecast`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name       | Type   | Meaning                                                   |
|------------|--------|-----------------------------------------------------------|
| `path`     | string | The file. May be a cloud URL.                             |
| `open_tab` | string | Use an open GUI tab instead (name, or `@active`).         |
| `table`    | string | Sheet or table name for multi-table sources.              |
| `x`        | string | Time column: dates, date-times or numbers, evenly spaced. |
| `y`        | string | Column of numbers to forecast.                            |
| `periods`  | number | How many periods ahead. Default `12`.                     |
| `season`   | number | Season length in points. Default: read from the spacing.  |

## Response

For `monthly_sales.csv` with `x: "month"`, `y: "sales"`, `periods: 1`:

```json
{
  "season": 12,
  "forecast": {
    "schema": [
      { "name": "x", "type": "Date32" },
      { "name": "forecast", "type": "Float64" },
      { "name": "lo80", "type": "Float64" },
      { "name": "hi80", "type": "Float64" },
      { "name": "lo95", "type": "Float64" },
      { "name": "hi95", "type": "Float64" }
    ],
    "rows": [["2026-01-01", 148.435, 146.325, 150.545, 145.208, 151.662]],
    "row_count": 1,
    "truncated": false,
    "total_rows_available": 1
  }
}
```

`season` is 0 when there was not enough history (two full cycles) for one.
Uneven x values are refused; bucket them first with `resample_timeseries`.
