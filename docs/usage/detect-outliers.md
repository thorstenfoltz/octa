# Detect Outliers

**Analyse > Detect outliers...** highlights numeric values that sit far from
the rest of their column, or from their neighbours in a series over time, painting each flagged cell **orange** so unusual
readings stand out.

## Methods

- **IQR (interquartile range)** - flags cells outside
  `[Q1 - k*IQR, Q3 + k*IQR]`. The usual `k` is `1.5`.
- **Z-score (standard deviations)** - flags cells more than `k` standard
  deviations from the mean. The usual `k` is `3`.
- **Neighbours (for a series over time)** - flags cells that differ from the
  values just before and after them. See below.

Tick the columns to scan (numeric columns are pre-selected) and set `k`,
then press **Detect**. Columns with fewer than four numbers are skipped.

## Unusual points in a series over time

IQR and Z-score look at a column as a whole. In a series that grows, or that
has a busy season, that hides exactly the values worth a look: a bad day in
December is still higher than a good day in February, so neither method
flags it. **Neighbours** compares each value with the values around it
instead.

- **Order by** is the date, time or number column that puts the rows in
  order. The rows need not be sorted, and a row without a value there is
  left out.
- **Points each side** is how many values before and after are compared.
  Empty means 7, so each value is judged against up to 14 neighbours. It
  needs at least four.
- **Same weekday only** compares a Sunday with the Sundays before and after
  it, a Monday with Mondays. Use it for daily data with a weekly rhythm,
  where a quiet Sunday is normal and a quiet Tuesday is not. The points each
  side then count weeks.
- A value is flagged when it lies more than `k` robust deviations from its
  neighbours' median. The usual `k` is `3`. The deviation is 1.4826 times
  the median distance from that median, so one extreme neighbour does not
  widen it. Where the neighbours are all the same, any other value is
  flagged.

An example: visitors per day, **Order by** `day`, **Points each side** 4.

| day        | visitors |
|------------|----------|
| 2024-06-05 | 103      |
| 2024-06-06 | 99       |
| 2024-06-07 | 240      |
| 2024-06-08 | 102      |
| 2024-06-09 | 97       |

The days around 7 June all lie near 100, so 240 is flagged and nothing else
is. In a series that climbs from 100 to 1,000 over a year, the same jump on an
early day would be flagged here too, while IQR and Z-score would let it pass
as an ordinary value of the column.

The three controls are greyed for the other two methods. The order column
itself is never scanned.

## What Detect does

Choose under **When done**:

- **Highlight outlier cells** - paints each flagged cell **orange**. This is
  **per tab and session-only**: it never changes the data, only how it is
  shown, and **Clear highlight** removes it. Manual colour marks, conditional
  colours, and validation highlights all take priority over the orange.
- **Add an is_outlier column** - appends a boolean `is_outlier` column that
  is `true` for every row holding at least one flagged value. This is a real,
  undoable edit (Ctrl+Z) you can save, sort, or filter on.

## Stepping through flagged cells

With **Highlight** chosen, <kbd>F10</kbd> jumps to the next flagged cell
and <kbd>Shift</kbd>+<kbd>F10</kbd> to the previous one, wrapping at the
ends, with a `Problem 3 of 27` counter in the status bar. The same keys
also cover [data validation](data-validation.md) violations, and rows
hidden by the current filter are skipped.

## Command line and MCP

Also available as `octa --outliers` (see the
[`--outliers`](../cli/outliers.md) reference) and as the
[`detect_outliers`](../mcp/tools/detect_outliers.md) MCP / assistant tool.
