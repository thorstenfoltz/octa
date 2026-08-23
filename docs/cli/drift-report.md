# `--drift-report`

Compare two versions of the same dataset and report how it moved.

```bash
octa --drift-report yesterday.parquet today.parquet
octa --drift-report a.csv b.csv --fail-on null_rate:0.05,rows:0.1
octa --drift-report a.csv b.csv -f json
```

Columns are matched by name. One present on a single side is reported as
added or removed and produces no metric rows. Every shared column gets
its null rate, distinct count and, when both sides are numeric, its
minimum, maximum and mean compared. Columns with few enough distinct
values also list the category values that appeared and vanished.

The report goes to stdout in the chosen `-f` format; the row counts, the
added and removed column names and the pass/fail summary go to stderr,
so a pipe stays parseable.

## Output

| Column     | Meaning                                                                                      |
|------------|----------------------------------------------------------------------------------------------|
| `column`   | Source column, empty for the table-wide `rows` row                                           |
| `metric`   | `rows`, `null_rate`, `distinct_count`, `min`, `max`, `mean`, `new_values`, `vanished_values` |
| `before`   | The earlier value, or the vanished category list                                             |
| `after`    | The later value, or the new category list                                                    |
| `change`   | Relative change, when both sides are numbers                                                 |
| `breached` | Whether this row failed a `--fail-on` gate                                                   |

## Gating a pipeline

```bash
octa --drift-report baseline.parquet daily.parquet --fail-on null_rate:0.05
```

`--fail-on` takes a comma-separated list of `metric:change` pairs, where
`change` is the largest relative move that still passes. The command
exits **1** when any gate is breached and **0** otherwise, so a CI step
can depend on it directly. Without `--fail-on` the report is
informational and always exits 0.

A baseline of zero that moved at all counts as an unbounded change. That
is deliberate: a null rate going from 0 to 0.5 is exactly the case a
gate exists to catch, and a relative change is undefined there.

A gate naming a metric that never appears in the report is not an error
and does not fail the run.

Every figure comes from the same Summary pass the **Analyse -> Summary**
tab uses, so the command line and the window can never disagree. See
[Data Drift](../usage/data-drift.md) for the dialog.
