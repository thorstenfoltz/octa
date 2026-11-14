# Chart

Render the active table as an interactive chart. Charts open as their
own tabs so you can keep the source table open side-by-side and have
several charts of the same data running at once.

Trigger via:

- **Analyse → Chart...** in the toolbar
- <kbd>F5</kbd>

The entry is hidden on string-only tables since there's nothing to
plot. The chart view only opens for a table that has at least one
numeric column.

![Chart tab](../assets/screenshots/chart-tab-overview.png)

## Chart kinds

Pick the chart kind in the leftmost combo of the control bar. Each
kind expects a specific column shape:

| Kind      | X column                  | Y column(s)         | Notes                                                                                 |
|-----------|---------------------------|---------------------|---------------------------------------------------------------------------------------|
| Histogram | Numeric / Date / DateTime | *(none)*            | Frequency count of X values, binned via [Sturges' rule](#histogram-bins).             |
| Bar       | Categorical (or numeric)  | One or more numeric | Groups rows by X, aggregates Y(s) via the chosen [aggregation](#bar-aggregations).    |
| Line      | Numeric / Date / DateTime | One or more numeric | One polyline per Y column. Points auto-sort by X.                                     |
| Scatter   | Numeric / Date / DateTime | One or more numeric | Disconnected point cloud, one series per Y column.                                    |
| Box       | *(none)*                  | One or more numeric | Tukey 5-number summary per Y column.                                                  |
| Heatmap   | Categorical               | One numeric         | One coloured cell per X and Rows pair. See [Heatmap](#heatmap).                       |
| Calendar  | Date / DateTime           | One numeric         | One coloured square per day, a band per year. See [Calendar](#calendar).              |
| Pareto    | Categorical (or numeric)  | One numeric         | Bars sorted largest first plus the running share of the total. See [Pareto](#pareto). |

## Heatmap

A grid of coloured cells: the **X:** column makes the columns, the
**Rows:** column makes the rows, and each cell is the **Agg:**
aggregate (Sum, Avg, Count, Min, Max) of the value column for that
pair. The value is the first column ticked in **Y:**; with **Count**
no value column is needed. Pairs that never occur stay empty.

- **Colour scale**: one blue ramp from light (smallest) to dark
  (largest) when the values are all on one side of zero. When they
  cross zero the scale turns blue (negative), white (zero), red
  (positive), with white always at 0. The colours are fixed, not
  taken from the theme, so the window and the export agree.
- **Labels**: a cell shows its value as text once it is big enough to
  hold it. Make the window larger, or chart fewer categories, to see
  them. Row and column names thin out the same way when they would
  overlap.
- **Hover**: a cell shows its X value, its Rows value, its value (a
  bare number, without the aggregation name) and how many rows went
  into it.
- **Category cap**: both X and Rows are limited by
  [`chart_max_categories`](../reference/settings.md#performance), which
  you can change in Settings. Above it the chart shows a message with
  the exact number instead of drawing; filter the table first or pick a
  column with fewer distinct values. A missing value is its own
  category, listed last.

## Calendar

One coloured square per day, laid out like a wall calendar: a column
per week, a row per weekday, weeks starting on Monday, and one band per
year with the newest year at the top. Pick a date or date-time column
as **X:**, a value column in **Y:** and an **Agg:**. Rows from the same
day are combined with the aggregate; with **Count** no value column is
needed. Days with no rows stay empty, as does a day whose value is empty
(for any aggregation but Count), and rows without a date are
skipped. If no row holds a date, the chart says the column has none.

The colours work as in the [heatmap](#heatmap). Hover a square to see
its date and value. The calendar has no category cap.

## Pareto

Bars sorted largest first, with a line for the running share of the
total: the picture behind "a few customers bring most of the revenue".
Pick the category as **X:**, one value column in **Y:** and an
**Agg:**, exactly as for Bar. Only the first ticked Y column is drawn.
The line ends at 100 percent, which sits level with the tallest bar,
and every point carries its percentage (up to 30 bars; beyond that the
labels would overlap, and the export's percent scale on the right edge
still reads them). The line is left out under a log scale and when a
bar is negative, since neither is a share of a total. The category cap
of Bar applies. To write the classes into the table, use the **ABC
class** kind of Calculated columns.

## Dates on the axes

`Date` columns are internally charted as **days since 1970-01-01** and
`DateTime` columns as **seconds since the Unix epoch**, but the X-axis
**tick labels are formatted back into dates** (`YYYY-MM-DD` for
`Date`, `YYYY-MM-DD HH:MM:SS` for `DateTime`). So a histogram of a
date column shows `2024-01-01 / 2024-01-15 / 2024-02-01` on the X
axis rather than the raw `19723` / `19737` / `19754` values. The
same formatting applies to Line, Scatter, and the exported PNG /
SVG / PDF.

Supported parse formats: ISO `YYYY-MM-DD`, dotted European
`DD.MM.YYYY`, slashed European `DD/MM/YYYY`, slashed US `MM/DD/YYYY`.
For timestamps add the time component (`YYYY-MM-DD HH:MM:SS`, ISO `T`
separator, optional fractional seconds, optional trailing `Z`).

## Categorical X axes

Bar charts (always) and Line / Scatter (when X is non-numeric)
treat the X column as **categories**. Each unique value gets one
tick labelled with the category name, in first-seen order. So a
bar of `country → population` shows `DE / US / JP / ...` on the X
axis, not `0 / 1 / 2 / ...`. Box plots use the Y column names as
their X-axis labels.

There's a configurable cap on distinct categories
([`chart_max_categories`](../reference/settings.md#performance),
default 200). Above it the chart refuses to draw and shows an
inline error asking you to filter or aggregate the table first.
The [Heatmap](#heatmap) shares this cap, applied to X and Rows each.

For Line with categorical X, points sit at category indices in
encounter order and the line connects them left-to-right. The
result has the same shape as the Bar would draw, just stroked
instead of filled.

## Bar aggregations

| Aggregation | What it does                                    |
|-------------|-------------------------------------------------|
| Sum         | Add up every Y value in the group.              |
| Avg         | Arithmetic mean of every Y value in the group.  |
| Count       | Number of rows in the group (ignores Y values). |
| Min / Max   | Smallest / largest Y value in the group.        |

## Histogram bins

Default: **Sturges' rule** (`ceil(1 + log2(n))`, clamped to `[5, 50]`).
Untick **Auto (Sturges)** to set the bin count by hand (1 to 250).

## Customise

The **Customise** collapsible exposes presentation knobs:

- **Title**: free text rendered above the plot. Empty = no title.
- **X-axis label**: overrides the column name. Empty = use the column
  name.
- **Y-axis label**: same idea.
- **Legend**: Off / Top-left / Top-right / Bottom-left / Bottom-right.
- **Grid**: tick **Show grid lines** to render the background grid;
  untick for a clean white plot area.
- **Series**: per-Y-column **Label** override (used in legend +
  tooltip) and a custom **Colour** picker (tick **Custom** to override
  the auto-cycled palette colour).

### X axis

A dedicated **X axis** sub-section under Customise, mirroring the Y axis
controls so you can clamp either dimension:

| Control       | What it does                                                                                                                                                                                                                 |
|---------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Min / Max** | Force fixed left / right bounds. Both must be set; a half-set range is ignored. Leave blank to auto-fit. For **Date** axes the bound is in *days since 1970-01-01*; for **DateTime** axes in *seconds since the Unix epoch*. |
| **Step**      | Custom grid step in original-data units. Empty = let `Octa` pick.                                                                                                                                                            |

Categorical Bar and Box charts treat the bounds as *category indices*
(0, 1, 2, …), so you can zoom into a slice of bars by setting Min / Max.

### Y axis

A dedicated **Y axis** sub-section under Customise:

| Control           | What it does                                                                                                                                                                                               |
|-------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Min / Max**     | Force fixed lower / upper bounds. Both must be set; a half-set range is ignored. Click **Auto** to reset.                                                                                                  |
| **Step**          | Custom grid step in original-data units. Empty = let `Octa` pick.                                                                                                                                          |
| **Integers only** | Format Y-axis ticks as whole numbers (no `.0` suffix). Useful for counts.                                                                                                                                  |
| **Log scale**     | Apply `log10(...)` to the Y values before plotting. Non-positive values drop silently. The axis label gets a `(log10)` suffix and the tick formatter renders the original magnitudes (e.g. `100`, `1000`). |

When **Min / Max** are set with log scale on, the bounds are interpreted
in original-data units and projected into log10 space internally. Type
`10` and `10000` to get the familiar three-decade range without doing
the maths.

## Sampling

Histogram, Line, and Scatter are subject to
[`chart_max_points`](../reference/settings.md#performance)
(default 25,000). Above the cap the chart evenly-spaces samples
and a pill above the plot reads `Sampled 25,000 of 2,000,000 rows`.

Bar, Box, Heatmap and Calendar always work off the full input. Bar, Heatmap and Calendar can afford to because
they aggregate per category or day anyway, and Box because the 5-number
summary is cheap regardless of row count.

Even-spaced sampling will miss outliers. The right answer is to
**filter first** and then chart, or to bump the cap if you have the
memory.

## Exporting

Three buttons sit on the right of the row above the plot:

| Button         | What you get                                                                                     |
|----------------|--------------------------------------------------------------------------------------------------|
| **Export PDF** | One-page PDF rendered from the chart's SVG via `svg2pdf`. Print-quality vector.                  |
| **Export PNG** | 2x retina-resolution PNG (1600 x 1000 pixels) rendered by `resvg` from the same SVG.             |
| **Export SVG** | The hand-emitted SVG file directly. Editable in Inkscape / Illustrator, embeddable in web pages. |

Heatmap and Calendar export too. The exported calendar is simpler than
the window: it has no month or weekday labels and lists the years
oldest first.

All three formats are derived from the same SVG so they look the same
regardless of window size or DPI. Title / axis labels / legend /
per-series colours carry into every export.

## Interacting with the plot

| Gesture                  | Action                         |
|--------------------------|--------------------------------|
| Drag                     | Pan.                           |
| Mouse wheel              | Zoom in / out.                 |
| Right-drag a box         | Zoom into that region.         |
| Double-click             | Reset to auto-bounds.          |
| Hover over a point / bar | Show coordinates in a tooltip. |

## Trend, forecast and seasonality

Line charts over dates or numbers can draw a trend line, forecast each line
ahead with 80% and 95% ranges, and split each line into trend, repeating
pattern and rest; see [Trend, forecast and seasonality](forecast.md).

## On a live database

A chart opened from a database tab that holds only part of its table draws
over the whole table, asking the database, heatmaps and calendars included:
the grouping runs on the server and only the cells come back; see
[Analyses on live databases](analyses-on-live-databases.md#charts).

## See also

- [Search & Filter](search-and-filter.md) for narrowing the table
  before charting.
- [Value Frequency](value-frequency.md) gives a `value_counts()`-style
  view for a single column without leaving the table.
- [Settings → Performance](../reference/settings.md#performance) is
  where you configure `chart_max_points` and `chart_max_categories`.
