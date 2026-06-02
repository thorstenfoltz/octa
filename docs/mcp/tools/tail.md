# `tail`

Read a tabular data file and return its **last N rows**. The mirror of
the first-N-rows behaviour you get from [`read_table`](read_table.md)
with a `limit`; same response shape.

## When to use

When the interesting records are at the end of a file (the newest log
lines, the latest appended rows) and you don't want to pull the whole
table.

## Input schema

| Parameter   | Type   | Required? | Default               | Description                                                       |
|-------------|--------|-----------|-----------------------|-------------------------------------------------------------------|
| `path`      | string | yes       | (no default)          | Absolute or working-directory-relative path to the file           |
| `limit`     | int    | no        | server default (1000) | Number of trailing rows to return. `0` = the whole loaded window  |
| `table`     | string | no        | (no default)          | Specific table to read for multi-table sources                    |
| `unlimited` | bool   | no        | `false`               | Lift the 5,000,000-row file-loader cap so the true end is reached |

## Response shape

Identical to [`read_table`](read_table.md): `{ schema, rows,
row_count, truncated, total_rows_available, cell_truncated }`. The
`rows` are the last `limit` rows of the loaded window.

## Notes

- For streaming formats the file loads with the 5 M-row cap, so the
  tail reflects the end of that window. Pass `unlimited: true` to tail
  the genuine end of a very large file.
- For multi-table sources pass `table`.

## Example call

```json
{
  "name": "tail",
  "arguments": { "path": "/var/log/events.csv", "limit": 20 }
}
```

## See also

- [`read_table`](read_table.md): first-N / arbitrary rows.
- [`sample`](sample.md): a reproducible random sample.
- CLI [`octa --tail`](../../cli/tail.md).
