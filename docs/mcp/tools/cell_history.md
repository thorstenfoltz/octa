# `cell_history`

The Git history of one cell in a file inside a Git repository: the commits
that changed it, newest first. The same history as the GUI's
[Cell History](../../usage/cell-history.md) and `octa --cell-history`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name        | Type     | Meaning                                                     |
|-------------|----------|-------------------------------------------------------------|
| `path`      | string   | A file inside a Git repository.                             |
| `column`    | string   | The column of the cell.                                     |
| `key`       | string[] | Key column(s) that identify the row. Pair with `key_value`. |
| `key_value` | string[] | The row's value in each `key` column, same order.           |
| `row`       | number   | 1-based row number, when there is no key.                   |
| `depth`     | number   | How many commits to read, newest first. Default `50`.       |

## Response

```json
{
  "history": {
    "schema": [
      { "name": "commit", "type": "Utf8" },
      { "name": "date", "type": "Utf8" },
      { "name": "author", "type": "Utf8" },
      { "name": "subject", "type": "Utf8" },
      { "name": "value", "type": "Utf8" },
      { "name": "change", "type": "Utf8" }
    ],
    "rows": [
      ["a1b2c3d", "2026-09-20 14:02", "Tess", "raise price", "25", "changed"],
      ["9f8e7d6", "2026-09-18 09:40", "Tess", "create", "20", "earliest"]
    ],
    "row_count": 2,
    "truncated": false,
    "total_rows_available": 2
  },
  "positional_commits": [],
  "unreadable": [],
  "more": false
}
```

`change` is one of `changed`, `row_added`, `row_removed`, `column_added`,
`column_removed`, or `earliest` for the oldest version read. Uncommitted
changes on disk come first with `commit` set to `(not committed)`.
`positional_commits` lists versions where the key was not unique or missing,
so the row was matched by its position there. Renames are followed.
