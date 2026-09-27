# `--forecast`

Forecast one column over time with Holt-Winters: the level, the trend and a
repeating season, with 80% and 95% ranges. The same model as the chart's
Forecast; see [Trend and forecast](../usage/forecast.md).

```sh
octa --forecast monthly_sales.csv --forecast-x month --forecast-y sales
octa --forecast visits.csv --forecast-x day --forecast-y visits --forecast-periods 14 --forecast-season 7
```

## Flags

| Flag                 | Required? | Description                                               |
|----------------------|-----------|-----------------------------------------------------------|
| `--forecast`         | yes       | The file.                                                 |
| `--forecast-x`       | yes       | Time column: dates, date-times or numbers, evenly spaced. |
| `--forecast-y`       | yes       | Column of numbers to forecast.                            |
| `--forecast-periods` | no        | How many periods ahead. Default `12`.                     |
| `--forecast-season`  | no        | Season length in points. Default: read from the spacing.  |

## Output

For `samples/features/monthly_sales.csv` (the first two of the twelve rows):

```text
x           forecast  lo80     hi80     lo95     hi95
2026-01-01  148.435   146.325  150.545  145.208  151.662
2026-02-01  155.736   152.752  158.72   151.172  160.3
```

The season length used goes to stderr (`none` when there was not enough
history for one). The x values must be evenly spaced; bucket uneven data
first, for example with [`--resample`](timeseries.md).
