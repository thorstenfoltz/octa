# `calculate_column`

**Append one calculated column** (rank, row number, running total, percent of
total, change to the previous row, a neighbouring row's value, a visit number,
an ABC class) to a tabular file and write the result. Same
engine as **Columns > Calculated column...** in the app and `octa --calculate`.

This is a **write** tool, so it is removed under `--mcp-read-only`.

## When to use

- Ranking rows by a value, optionally restarting per group.
- Adding a running total or a month-on-month change to a time series.
- Putting the previous or next row's value beside each row (lag / lead).
- Numbering each user's visits from event timestamps (sessions).
- Labelling rows A, B or C by how much of the total they make up.

## Input schema

| Parameter     | Type     | Required? | Default                     | Description                                                                                                                                   |
|---------------|----------|-----------|-----------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------|
| `path`        | string   | yes       | (no default)                | Path to the source file                                                                                                                       |
| `kind`        | string   | yes       | (no default)                | `rank`, `dense_rank`, `row_number`, `running_total`, `percent_of_total`, `diff_prev`, `pct_change_prev`, `previous`, `next`, `session`, `abc` |
| `value`       | string   | no        | (none)                      | Column the calculation works on; not used by `row_number`                                                                                     |
| `order`       | string   | no        | file order                  | Row order for `row_number`, `running_total`, `diff_prev`, `pct_change_prev`, `previous`, `next`                                               |
| `descending`  | bool     | no        | ranks: `true`, else `false` | Largest first                                                                                                                                 |
| `groups`      | string[] | no        | `[]`                        | Columns whose values restart the calculation                                                                                                  |
| `offset`      | integer  | no        | `1`                         | `previous` / `next`: how many rows away                                                                                                       |
| `gap_minutes` | number   | no        | `30`                        | `session`: a pause longer than this starts a new visit                                                                                        |
| `abc_a`       | number   | no        | `80`                        | `abc`: class A ends at this running share of the total, in percent                                                                            |
| `abc_b`       | number   | no        | `95`                        | `abc`: class B ends here, the rest is C                                                                                                       |
| `new_column`  | string   | no        | `<value>_<kind>`            | Name of the new column; a taken name is refused                                                                                               |
| `output_path` | string   | no        | overwrite `path`            | Where to write the result; format follows its extension                                                                                       |
| `unlimited`   | bool     | no        | `false`                     | Lift the file-loader row cap so every row is read                                                                                             |

Percent of total is a percentage (12.5 means 12.5 %). Empty values rank last;
differences are empty at the start of a group and beside an empty value. For
`session`, `value` is the timestamp column and `groups` the user column; the
rows need not be sorted. `abc` sorts largest first and refuses negative values.
A file
read only in part (the row cap) is refused rather than written back with fewer
rows; pass `unlimited: true`. Database files (SQLite / DuckDB / GeoPackage) are
not valid sources or targets.

## Example

```json
{ "path": "/data/sales.csv", "kind": "running_total", "value": "amount", "order": "day" }
```

## Output

```json
{ "rows_written": 1200, "new_column": "amount_running_total", "output": "/data/sales.csv" }
```

## See also

- In the app: [Calculated columns](../../usage/calculated-columns.md)
- Command line: [`--calculate`](../../cli/calculate.md)
