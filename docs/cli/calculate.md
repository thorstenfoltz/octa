# `--calculate`

Add a calculated column and print the table to stdout. The input file is
never modified.

```
octa --calculate KIND FILE [--calc-column COL] [--order-by COL]
     [--partition-by-cols COLS] [--calc-descending | --calc-ascending]
     [--calc-name NAME] [--calc-offset ROWS] [--calc-gap MINUTES]
     [--abc-a PERCENT] [--abc-b PERCENT] [-f tsv|json|csv]
```

## Kinds

| Kind               | What the new column holds                                     |
|--------------------|---------------------------------------------------------------|
| `rank`             | Position by value, ties share a rank and leave a gap          |
| `dense_rank`       | Like `rank`, but without gaps after ties                      |
| `row_number`       | 1, 2, 3 ... in the chosen order (needs no `--calc-column`)    |
| `running_total`    | Sum so far, row by row                                        |
| `percent_of_total` | Each value as a share of the whole (or of its group)          |
| `diff_prev`        | Difference to the previous row                                |
| `pct_change_prev`  | Percent change to the previous row                            |
| `previous`         | The value `--calc-offset` rows before                         |
| `next`             | The value `--calc-offset` rows after                          |
| `session`          | Visit number per group from the timestamps in `--calc-column` |
| `abc`              | A, B or C by the running share of the total                   |

| Flag                  | Default           | What it does                                         |
|-----------------------|-------------------|------------------------------------------------------|
| `--calc-column`       | none              | The column to calculate on (not for `row_number`)    |
| `--order-by`          | file order        | The column that sets the row order                   |
| `--partition-by-cols` | none              | Comma-separated columns that restart the calculation |
| `--calc-descending`   | ranks only        | Order largest (or latest) first                      |
| `--calc-ascending`    | other kinds       | Order smallest first; a rank gives the smallest 1    |
| `--calc-name`         | `<column>_<kind>` | Name of the new column; must not exist yet           |
| `--calc-offset`       | 1                 | `previous` / `next`: how many rows away              |
| `--calc-gap`          | 30                | `session`: minutes of pause that start a new visit   |
| `--abc-a`             | 80                | `abc`: class A ends at this running share in percent |
| `--abc-b`             | 95                | `abc`: class B ends here, the rest is C              |

`--calc-descending` and `--calc-ascending` exclude each other. The `--calc-*`
and `--abc-*` flags only work together with `--calculate`.

For `session`, name the user column with `--partition-by-cols`; the rows need
not be sorted. `abc` refuses negative values and limits that do not rise.

## Examples

```
octa --calculate rank --calc-column sales --partition-by-cols region sales.csv
octa --calculate running_total --calc-column amount --order-by order_date orders.parquet -f csv
octa --calculate previous --calc-column amount --order-by month --calc-offset 12 monthly.csv
octa --calculate session --calc-column at --partition-by-cols user --calc-gap 20 clicks.csv
octa --calculate abc --calc-column revenue --abc-a 70 --abc-b 90 products.csv
```

## See also

- In the app: [Calculated columns](../usage/calculated-columns.md)
- MCP: [`calculate_column`](../mcp/tools/calculate_column.md)
