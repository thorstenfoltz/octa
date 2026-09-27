# `--cell-history`

The commits that changed one cell of a file kept in a Git repository, newest
first. The row is followed by a key, so a re-sorted file is not reported as
every row changing, and renames are followed. When the file on disk differs
from the last commit, its current state comes first as `(not committed)`.

```sh
octa --cell-history prices.csv --history-column price --history-key id --history-value 2
octa --cell-history prices.csv --history-column price --history-row 5
```

## Flags

| Flag               | Required? | Description                                                      |
|--------------------|-----------|------------------------------------------------------------------|
| `--cell-history`   | yes       | The file. Must be inside a Git repository.                       |
| `--history-column` | yes       | Column of the cell.                                              |
| `--history-key`    | one of    | Key column(s) that identify the row, comma-separated.            |
| `--history-value`  | with key  | The row's value in each key column, comma-separated, same order. |
| `--history-row`    | one of    | 1-based row number, when there is no key.                        |
| `--history-depth`  | no        | How many commits to read, newest first. Default `50`.            |

## Output

```text
commit   date              author  subject      value  change
a1b2c3d  2026-09-20 14:02  Tess    raise price  25     changed
9f8e7d6  2026-09-18 09:40  Tess    create       20     earliest
```

`change` is one of `changed`, `row_added`, `row_removed`, `column_added`,
`column_removed`, or `earliest` for the oldest version read. On stderr: the
versions where the key was not unique or missing (the row was matched by its
position there), versions that could not be read, and a note when older
commits exist beyond `--history-depth`.

See [Cell History](../usage/cell-history.md).
