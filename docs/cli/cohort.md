# `--cohort`

Build a cohort table from a file and print it to stdout: who first appeared
in which period, and how many came back (or a column's sum) in each later
period. The input file is never modified.

```
octa --cohort FILE --cohort-who COL --cohort-when COL
     [--cohort-period day|week|month|quarter|year]
     [--cohort-show share|count|sum] [--cohort-sum COL] [-f tsv|json|csv]
```

| Flag              | Default | What it does                                             |
|-------------------|---------|----------------------------------------------------------|
| `--cohort-who`    | none    | The column naming a person or customer. Required.        |
| `--cohort-when`   | none    | The date or timestamp column. Required.                  |
| `--cohort-period` | `month` | `day`, `week` (Monday start), `month`, `quarter`, `year` |
| `--cohort-show`   | `share` | `share` (percent), `count` (people) or `sum`             |
| `--cohort-sum`    | none    | The number column `--cohort-show sum` adds up            |

The five `--cohort-*` flags only work together with `--cohort`.

Output columns: `cohort`, `size`, then `<period>_0`, `<period>_1`, ... A
cell for a period the data does not reach yet is empty. Rows with no who or
no readable date are left out, and their number is printed on stderr.

## Example

```
$ octa --cohort orders.csv --cohort-who customer --cohort-when ordered_at
cohort size month_0 month_1 month_2
2024-01 2 100.0 50.0 50.0
2024-02 1 100.0 0.0
```

## See also

- In the app: [Cohorts](../usage/cohorts.md)
- MCP: [`cohort_table`](../mcp/tools/cohort_table.md)
