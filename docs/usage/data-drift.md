# Data Drift

<!-- SCREENSHOT: data-drift-dialog.png: The Data drift dialog. Before / After each with the "Open tab" radio selected and a tab chosen in the dropdown, the Category limit field showing 50, and the Compare button enabled at the bottom. -->

**Analyse -> Data drift...** compares two versions of the same data and
tells you what moved. Not which rows changed, which is what
[Compare](view-modes/compare.md) and `--diff` answer, but whether the
shape of the data still looks the same: are the same columns there, did
a column start arriving empty, did a category disappear.

That is the question worth asking about yesterday's extract against
today's, before anything downstream trusts it.

## What it compares

Columns are matched by **name**. One present on a single side is
reported as added or removed and produces no other rows: there is
nothing to compare it against, and inventing a zero baseline would make
every threshold fire.

For each shared column:

| Metric            | Meaning                                            |
|-------------------|----------------------------------------------------|
| `null_rate`       | Share of missing values, 0 to 1                    |
| `distinct_count`  | How many different values the column holds         |
| `min`, `max`      | Numeric columns only, and only when both sides are |
| `mean`            | Numeric columns only                               |
| `new_values`      | Category values that appeared, named               |
| `vanished_values` | Category values that disappeared, named            |

Plus one `rows` row for the table as a whole.

The category rows only appear for columns with few enough distinct
values, because listing the new entries of a free-text column is noise
rather than drift. The limit is the **Category limit** field in the
dialog, 50 by default.

Every number comes from the same Summary pass the
[Summary](summary.md) tab uses, called once per side. Nothing here
computes a statistic of its own, so the two can never disagree.

## Reading the result

One row per column per metric. Hovering a header shows the same
explanation in the app.

| Column     | What it holds                                          |
|------------|--------------------------------------------------------|
| `column`   | The column the row is about; blank for table-wide rows |
| `metric`   | Which measurement, from the table above                |
| `before`   | The value in the Before table                          |
| `after`    | The value in the After table                           |
| `change`   | `(after - before) / before`; see below                 |
| `breached` | Whether this row crossed a threshold you set           |

**`change` is relative, not absolute.** `0.1` means the value moved by a
tenth of what it was, whether that is 5 rows out of 50 or 5 million out
of 50 million. That is what makes one threshold usable across columns of
very different size.

Three cases to know:

- It is **blank** where a relative change is meaningless - a list of new
  category values has nothing to divide by.
- A **baseline of 0 that moved at all** counts as an unbounded change,
  so it breaches any threshold. A null rate going from 0 to 0.5 is
  infinitely worse in relative terms, and reporting `0` there would hide
  the most interesting kind of drift.
- It can be **negative**: the value went down.

`breached` is `false` for every row unless you set thresholds with
`--fail-on`; the dialog itself does not set any, so a comparison run
from the window reports movement without judging it.

## Using it

1. **Analyse -> Data drift...**
2. Pick a source on each side: an open tab, or a file from disk. The
   older version goes in **Before**.
3. **Compare** opens the result in a new tab.

<!-- TODO screenshot: the Data drift dialog with a before and an after side
     chosen. Listed in docs/assets/screenshots/INDEX.md. -->

## From the command line

```bash
octa --drift-report yesterday.parquet today.parquet
```

The report goes to stdout in whichever `-f` format you asked for; the
row counts, the added and removed column names and the summary line go
to stderr, so a pipe stays parseable.

Add `--fail-on` to make it a gate:

```bash
octa --drift-report yesterday.parquet today.parquet \
     --fail-on null_rate:0.05,rows:0.1
```

Each pair is a metric and the largest relative change that still passes.
The command exits **1** when any gate is breached, so a CI step can
depend on it. A baseline of zero that moved at all counts as an
unbounded change, which is what makes a null rate going from 0 to 0.5
fail every threshold rather than none.

## For the assistant

The `data_drift` tool answers the same question, with a `failed` flag
you can gate on. See the [MCP reference](../mcp/index.md).

## Limits

- Columns are matched by name only. A renamed column reads as one
  removed and one added.
- Both sides are read under the usual row cap, so a comparison of two
  very large files describes the rows that were loaded.
- Category lists name at most ten values, then say how many more there
  were.
