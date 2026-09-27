# `--overlaps`

Find rows whose time spans overlap: two bookings of one room, one person
on two shifts. Prints one row per overlapping pair and **exits 1 when any
is found**, so a script can gate on it.

```sh
octa --overlaps bookings.csv --overlaps-lane room --overlaps-label title
```

Spans that only touch (one ends at 11:00, the next starts at 11:00) do not
overlap. A row without an end is a point, and overlaps a span it falls
strictly inside.

## Flags

| Flag               | Required? | Description                                                                   |
|--------------------|-----------|-------------------------------------------------------------------------------|
| `--overlaps`       | yes       | The file.                                                                     |
| `--overlaps-start` | no        | Start column. Default: the first date column.                                 |
| `--overlaps-end`   | no        | End column. Default: the second date column; with none, every row is a point. |
| `--overlaps-lane`  | no        | Only rows with the same value here can overlap (a room, a person).            |
| `--overlaps-label` | no        | A column shown beside each row of a pair.                                     |

## Output

```text
room row_a title_a start_a end_a row_b title_b start_b end_b
Room A 1 Team meeting 2026-10-05 09:00 2026-10-05 10:30 2 Customer call 2026-10-05 10:00 2026-10-05 11:00
```

The columns are named after the ones you picked: the lane column, then
row number, label, start and end for `_a` (the row of the pair that starts
first) and `_b` (the row that starts while `_a` is still running). A column
you did not pick gets a plain name (`lane`, `label_a`). Rows are numbered
from 1. The summary, and the rows that end before they
start (left out), go to stderr.

## Exit codes

| Code | Meaning                                                   |
|------|-----------------------------------------------------------|
| `0`  | No overlap.                                               |
| `1`  | At least one overlap, or the file or a column is missing. |

See [Timeline](../usage/view-modes/timeline.md).
