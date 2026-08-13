# `--resample` / `--rolling`

Two time-series reshapes, printed as a table like every other read action.
The input file is never modified.

```
octa --resample COL --value-cols COLS FILE [--interval UNIT] [--agg FN] [--group-by COLS]
octa --rolling  COL --order-by COL --window N FILE [--agg FN] [--partition-by-cols COLS]
```

Both build DuckDB SQL with the same builders as the GUI
[Time series](../usage/time-series.md) dialog and the
[`resample_timeseries`](../mcp/tools/resample_timeseries.md) /
[`rolling_window`](../mcp/tools/rolling_window.md) MCP tools, so all three
surfaces produce identical results.

## `--resample`: time buckets

Group rows into one bucket per interval of a timestamp column and aggregate.

| Flag                | Required? | Default | Description                                           |
|---------------------|-----------|---------|-------------------------------------------------------|
| `--resample COL`    | yes       |         | The timestamp column to bucket                        |
| `--value-cols COLS` | yes       |         | Comma-separated columns to aggregate                  |
| `--interval UNIT`   | no        | `day`   | `minute`/`hour`/`day`/`week`/`month`/`quarter`/`year` |
| `--agg FN`          | no        | `sum`   | `sum`/`mean`/`min`/`max`/`count`/`first`/`last`       |
| `--group-by COLS`   | no        |         | Comma-separated. One series per combination           |

The bucket lands in a column named `bucket` (or `bucket_2` if the source
already has one).

```
octa --resample day --interval month --value-cols amount sales.csv
octa --resample ts --interval week --agg mean --value-cols amount,qty \
     --group-by region sales.parquet
```

The timestamp column is cast with `TRY_CAST`, so one malformed value puts
that row in a null bucket rather than failing the whole query.

## `--rolling`: rolling window

Add a column holding the aggregate of the current row and the N-1 before it.

| Flag                       | Required? | Default | Description                                      |
|----------------------------|-----------|---------|--------------------------------------------------|
| `--rolling COL`            | yes       |         | The column aggregated                            |
| `--order-by COL`           | yes       |         | The column that orders the frame                 |
| `--window N`               | yes       |         | Rows in the frame, **including** the current one |
| `--agg FN`                 | no        | `mean`  | `sum`/`mean`/`min`/`max`/`count`/`first`/`last`  |
| `--partition-by-cols COLS` | no        |         | Comma-separated. Restarts the frame              |

Output is every source column plus `<COL>_rolling_<N>`.

```
octa --rolling amount --order-by day --window 7 --agg mean sales.csv
octa --rolling amount --order-by day --window 7 \
     --partition-by-cols region sales.csv
```

`--order-by` is required and cannot be omitted: a rolling aggregate over
unordered rows is meaningless.

Note the flag is `--partition-by-cols`, not `--partition-by` -- the latter is
the [split-into-files action](partition.md).

## Errors

Missing companion flags fail with exit 1 and name the flag, rather than
falling back to a silent default:

```
$ octa --rolling amount data.csv
error: --rolling requires --order-by COL
```

An unknown `--interval` or `--agg` lists the accepted words.

## See also

- [Time Series](../usage/time-series.md): the GUI dialog, with a live preview.
- [`--sql`](sql.md): the same thing written by hand, when the flags do not fit.
