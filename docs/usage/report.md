# Report

A report turns the table you are looking at into one HTML file you can
send to somebody: per-column statistics, a chart for each column, the
most common values and a correlation matrix.

The file is **self-contained**. Its styling is inline, its charts are
inline SVG, it runs no JavaScript and it fetches nothing, so it opens
from a mail attachment on a machine with no internet and can be published
as a static file with no assets beside it.

## Opening it

**File → Report...**. There is no default keyboard shortcut; one can
be assigned under **Settings → Shortcuts**.

The report covers the **active tab**, follows whatever filter is applied
to it, and includes your unsaved edits, exactly as the
[Summary](summary.md) tab does.

## What goes in

Each section can be switched off.

| Section                 | What it shows                                                                  |
|-------------------------|--------------------------------------------------------------------------------|
| **Column statistics**   | Type, nulls, unique values and the numeric summary for every column.           |
| **Distribution charts** | A histogram per numeric column, a bar chart of the commonest values otherwise. |
| **Most common values**  | The most frequent values per column, with counts and shares.                   |
| **Correlation**         | How the numeric columns move together.                                         |

Nothing here is calculated a second time. The statistics come from the
same engine as the [Summary](summary.md) tab, the top values from
[Value frequency](value-frequency.md), the matrix from
[Correlation](correlation.md), and the pictures from the
[Chart](chart.md) tab's own SVG export. The report cannot disagree with
what Octa shows on screen.

Distributions draws at most 50 columns and then says how many it left
out, since a 500-column table would otherwise produce a document nobody
scrolls. Correlation needs at least two numeric columns; below that the
section is left out rather than showing a column's correlation with
itself.

## Profiling a sample

**Profile a sample only** is off, so every row is examined. Turn it on
for a faster, approximate report on a very large table. The document then
states how many rows it looked at and how many there were, so nobody
mistakes approximate numbers for exact ones.

Under an active filter the sample is drawn from the rows the filter
leaves visible, never from the hidden ones.

## When it is done

Building runs in the background, so Octa stays usable, and **Cancel**
stops it. When it finishes the dialog shows where the file went and
offers **Open in browser**.

## Elsewhere in Octa

The same engine runs on the command line and over MCP:

```bash
octa --report sales.html sales.parquet
octa --report peek.html huge.parquet --report-sample 10000
```

See [`octa --report`](../cli/report.md) and the
[`create_report`](../mcp/tools/create_report.md) MCP tool, which the
in-app Assistant can also call.

## See also

- [Summary](summary.md), the statistics section as a tab.
- [Data quality report](data-quality-report.md), which grades a table
  rather than describing it.
- [Chart](chart.md), for one picture rather than a whole document.
