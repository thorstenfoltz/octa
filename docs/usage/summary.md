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

The statistics come from DuckDB's `SUMMARIZE`, so the exact column set
can vary slightly between DuckDB releases. Typical columns:

| Column            | Meaning                                              |
|-------------------|------------------------------------------------------|
| `column_name`     | The source column this row describes.                |
| `column_type`     | The SQL type DuckDB inferred for it.                 |
| `min` / `max`     | Smallest and largest value (lexicographic for text). |
| `approx_unique`   | Approximate count of distinct values.                |
| `avg` / `std`     | Mean and standard deviation (numeric columns only).  |
| `q25 / q50 / q75` | Quartiles (numeric columns only).                    |
| `count`           | Number of non-null values.                           |
| `null_percentage` | Share of null values in the column.                  |

## Working with the result

The Summary tab is an ordinary table tab: you can sort it, filter it,
copy cells, and export it via **File -> Save As**. It is a detached
snapshot with no source path, so it can never overwrite the original
file. Re-run **Analyse -> Summary...** after further edits to get a
fresh snapshot.

For a deeper look at a single column, use
[Value Frequency](value-frequency.md) instead.
