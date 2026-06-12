# Summary

The Summary tab answers "what does this table look like?" in one click.
It is the GUI counterpart of the CLI's `octa --describe` and of pandas'
`df.describe()`: one row of statistics per column of the active table.

<!-- SCREENSHOT: summary-overview.png: Summary tab showing per-column statistics (min, max, uniques, average, quartiles, null percentage) for a mixed-type table. -->
![Summary](../assets/screenshots/summary-overview.png)

## Opening it

**Analyse -> Summary...** opens a new tab named `Summary - <file>` for
the active table. Unsaved cell edits are included: the statistics
describe the table as you currently see it, not the file on disk.

## What it shows

One row per source column. Column titles are shown in your chosen
language, and hovering a title explains what the statistic means. The
available statistics are:

| Column            | Meaning                                              |
|-------------------|------------------------------------------------------|
| Column            | The source column this row describes (always shown). |
| Type              | The data type Octa inferred for it (always shown).   |
| Min / Max         | Smallest and largest value.                          |
| Mean / Median     | Average and middle value (numeric columns).          |
| Std dev           | Standard deviation (numeric columns).                |
| Q25 / Q75         | Lower and upper quartiles (numeric columns).         |
| Not null / Nulls  | Counts of present and missing values.                |
| Null %            | Share of missing values in the column.               |
| Unique            | Exact count of distinct values (nulls excluded).     |
| Distinct ratio    | Unique values divided by total rows.                 |
| Total rows        | Row count of the whole table.                        |

## How Min / Max work for text

For numbers, dates, and times, **Min** and **Max** are the smallest and
largest values, as you'd expect. For **text** columns the comparison is
*lexicographic* (dictionary order by character code), not by length or
meaning:

- It compares character by character, left to right.
- It is **case-sensitive**, and uppercase letters come before lowercase
  ones, so `"Zebra"` sorts before `"apple"`.
- Digits compare by their character, not their numeric value, so as text
  `"10"` sorts before `"9"` (the character `1` comes before `9`). Numbers
  stored as text do **not** sort numerically.

This matches DuckDB's default string ordering (a plain byte / code-point
comparison with no locale collation), since the figures come from
DuckDB's `SUMMARIZE`. If a column should sort numerically or by date,
give it a numeric or date type (Octa's
[date inference](../getting-started/first-steps.md) and the
[SQL view's](sql.md) `CAST` can help) rather than leaving it as text.

## Choosing which statistics show

**Settings -> Summary** has a checkbox per statistic. Turn off the ones
you don't need and the Summary tab drops those columns; Column and Type
are always present. The figures come from a single DuckDB `SUMMARIZE`
pass, plus derived null counts and an exact distinct-value count
(`COUNT(DISTINCT ...)`, so **Unique** never exceeds the row count).

## Working with the result

The Summary tab is an ordinary table tab: you can sort it, filter it,
copy cells, and export it via **File -> Save As**. It is a detached
snapshot with no source path, so it can never overwrite the original
file. Re-run **Analyse -> Summary...** after further edits to get a
fresh snapshot.

For a deeper look at a single column, use
[Value Frequency](value-frequency.md) instead.
