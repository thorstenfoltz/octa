# `--funnel`

Count how many people get through a series of steps in a file, in order, and
print one row per step to stdout. The input file is never modified.

```
octa --funnel FILE --funnel-who COL --funnel-event COL --funnel-steps A,B,C
     [--funnel-when COL] [--funnel-within-days DAYS] [-f tsv|json|csv]
```

| Flag                   | Default   | What it does                                                                                           |
|------------------------|-----------|--------------------------------------------------------------------------------------------------------|
| `--funnel-who`         | none      | The column naming a person or customer. Required.                                                      |
| `--funnel-event`       | none      | The column saying what happened in each row. Required.                                                 |
| `--funnel-steps`       | none      | The event values that make up the funnel, in order, comma-separated. Two or more, each once. Required. |
| `--funnel-when`        | row order | The date or timestamp column. Without it the rows count in file order.                                 |
| `--funnel-within-days` | no limit  | Count a later step only within this many days of the first step. Needs `--funnel-when`.                |

The five `--funnel-*` flags only work together with `--funnel`.

A person reaches a step when they have that event at or after the moment
they reached the step before.

Output columns: `step`, `users`, `share_of_first`, `share_of_previous`
(percent) and `dropped` (people lost since the step before). Rows with no who
or no readable date are left out, and their number is printed on stderr.

## Example

```
$ octa --funnel events.csv --funnel-who user --funnel-event event \
       --funnel-when at --funnel-steps view,cart,pay
step users share_of_first share_of_previous dropped
view 3 100.0 100.0 0
cart 2 66.7 66.7 1
pay 1 33.3 50.0 1
```

## See also

- In the app: [Funnel](../usage/funnel.md)
- MCP: [`funnel`](../mcp/tools/funnel.md)
