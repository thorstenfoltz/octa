# `--seasonality`

Split one column over time into its trend, its repeating pattern and the
rest, to tell a spike that is unusual from one that comes every season. The
same split as the chart's **Seasonality to table**; see
[Trend, forecast and seasonality](../usage/forecast.md#seasonality-to-table).

```sh
octa --seasonality monthly_sales.csv --seasonality-x month --seasonality-y sales
octa --seasonality visits.csv --seasonality-x day --seasonality-y visits --seasonality-season 7
```

## Flags

| Flag                   | Required? | Description                                               |
|------------------------|-----------|-----------------------------------------------------------|
| `--seasonality`        | yes       | The file.                                                 |
| `--seasonality-x`      | yes       | Time column: dates, date-times or numbers, evenly spaced. |
| `--seasonality-y`      | yes       | Column of numbers to split.                               |
| `--seasonality-season` | no        | Season length in points. Default: read from the spacing.  |

## Output

One row per point. For `samples/features/monthly_sales.csv` (the first row and
the first row with a trend):

```text
x           value  trend   seasonal  rest
2022-01-01  98.4           -16.472
2022-07-01  142.0  123.25  17.972    0.778
```

`trend` is the average over one whole season around the point, so it and
`rest` are empty for the first and last half season. `seasonal` is the
repeating pattern and `rest` is `value - trend - seasonal`.

The season length and how much of the movement around the trend the pattern
explains go to stderr:

```text
season length: 12, the pattern explains 100% of the movement around the trend
```

The season is read from the spacing (24 hourly, 7 daily, 52 weekly, 12
monthly, 4 quarterly); yearly data and plain numbers need
`--seasonality-season`. Two full seasons of history are needed, and the x
values must be evenly spaced; bucket uneven data first, for example with
[`--resample`](timeseries.md). Either problem exits 1 with the reason.

## See also

- MCP: [`seasonality`](../mcp/tools/seasonality.md)
- In the app: [Seasonality to table](../usage/forecast.md#seasonality-to-table)
