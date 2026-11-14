# Trend, forecast and seasonality

A **Line** chart over dates (or plain numbers) can draw a trend over each line,
forecast it ahead and split it into its trend and its repeating pattern. The controls sit in the row above the plot and only
appear for Line charts.

![A Line chart of samples/features/monthly_sales.csv with Trend set to Straight line and Forecast 12: the sales line, its straight trend, and the forecast continuing the summer peak with the darker 80% band inside the lighter 95% band.](../assets/screenshots/forecast-chart.png){ .screenshot-placeholder }

## Trend

| Trend              | What it draws                                                                                                                     |
|--------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| **None**           | Nothing.                                                                                                                          |
| **Straight line**  | The straight line that fits the points best (least squares).                                                                      |
| **Moving average** | At each point, the average of the last few: one season when the dates show one (7 for daily, 12 for monthly data), else 5 points. |

The trend is drawn as an extra series named `<series> (trend)`.

## Forecast

Type how many periods to forecast into **Forecast**; 0 turns it off. Each line
gets a continuation, `<series> (forecast)`, starting from its last real point,
with two shaded ranges around it:

- the darker band is the **80% range**: the value is expected to land inside
  it four times out of five;
- the lighter band is the **95% range**.

Both widen the further ahead the forecast goes. The model is Holt-Winters:
it follows the level, the trend and a repeating season, and picks its own
smoothing from the history. There is nothing to tune.

### Seasons

The season is read from the spacing of the dates:

| Spacing   | Season length |
|-----------|---------------|
| Hourly    | 24            |
| Daily     | 7             |
| Weekly    | 52            |
| Monthly   | 12            |
| Quarterly | 4             |
| Yearly    | none          |

A season is only used with at least two full cycles of history; with less,
the forecast follows the level and trend alone. Monthly, quarterly and yearly
forecasts step by calendar months, so they land on the same day of the month
as the data. Under **Advanced**, untick **Season length from the dates** to set
the length yourself.

### Evenly spaced points

A forecast needs at least 4 points, evenly spaced. Transactions with gaps
between them are not a series yet: bucket them first with **Analyse -> Time
series** (say, a sum per month), chart that, then forecast. The chart says so
when the spacing is uneven.

## Forecast to table

**Forecast to table** opens the forecast of each line in a new tab: one row
per period with `x`, `forecast`, `lo80`, `hi80`, `lo95` and `hi95`, ready to
save or copy.

## Seasonality to table

Is a spike in December unusual, or does December always look like that?
**Seasonality to table** answers it by splitting each line into three parts,
each a column in a new tab, with a second tab beside it that draws them as a
line chart:

| Column     | What it holds                                                                                                                |
|------------|------------------------------------------------------------------------------------------------------------------------------|
| `value`    | The line itself.                                                                                                             |
| `trend`    | The average over one whole season around each point. Empty for the first and last half season.                               |
| `seasonal` | The repeating pattern: how far each position of the season (each month, each weekday) usually sits above or below the trend. |
| `rest`     | What neither explains: `value - trend - seasonal`. A large rest is the unusual part.                                         |

The season length is read from the spacing of the dates, as for the forecast
(see [Seasons](#seasons)), or taken from **Advanced** when you set it there.
Yearly data and plain numbers name no season, so set one under **Advanced**
for those. The split needs at least two full seasons of history and evenly
spaced points; when it cannot run, the reason is shown under the row.

Both tabs' hint says how much of the movement around the trend the
pattern explains, from 0% (no pattern at all) to 100% (the line repeats
exactly). This is the classical additive decomposition.

An example, from `samples/features/monthly_sales.csv` (four years of monthly
sales with a summer peak), `month` as X and `sales` as Y:

| x          | value | trend  | seasonal | rest  |
|------------|-------|--------|----------|-------|
| 2022-01-01 | 98.4  |        | -16.472  |       |
| 2022-06-01 | 145.4 |        | 25.528   |       |
| 2022-07-01 | 142   | 123.25 | 17.972   | 0.778 |
| 2022-08-01 | 135.6 | 124.35 | 10.528   | 0.722 |

A January usually sits about 16 below the trend and a June about 26 above it,
so a high June is the season, not news. July 2022 is 142, of which the trend
explains 123.25 and the season 17.972, leaving a rest of 0.778: nothing
unusual. The first six months have no trend yet, since half a season of
history is missing before them.

## Exports

PNG, SVG and PDF exports include the trend and forecast lines. The shaded
bands are on-screen only; exports draw the 95% range as two lines,
`(95% low)` and `(95% high)`. On a log-scaled Y axis the bands are left out
too, since a range can dip below zero.

## Try it

`samples/features/monthly_sales.csv` has four years of monthly sales with a
summer peak. Open it, choose **Analyse -> Chart...**, pick **Line** with `month`
as X and `sales` as Y, and set **Forecast** to 12.

## Command line and MCP

`octa --forecast` (see [`--forecast`](../cli/forecast.md)) and the
[`forecast`](../mcp/tools/forecast.md) MCP tool use the same model;
`octa --seasonality` (see [`--seasonality`](../cli/seasonality.md)) and the
[`seasonality`](../mcp/tools/seasonality.md) MCP tool make the same split as
**Seasonality to table**.
