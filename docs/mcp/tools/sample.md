# `sample`

Read a tabular data file and return a **random N-row sample**. Sampling
is without replacement, the chosen rows keep their original order, and
the draw is **reproducible** for a given `seed`. Same response shape as
[`read_table`](read_table.md).

## When to use

To give Claude a representative slice of a large table without the bias
of always taking the first N rows (which over-represents whatever the
file happens to be sorted by).

## Input schema

| Parameter   | Type   | Required? | Default               | Description                                                       |
|-------------|--------|-----------|-----------------------|-------------------------------------------------------------------|
| `path`      | string | yes       | (no default)          | Absolute or working-directory-relative path to the file           |
| `limit`     | int    | no        | server default (1000) | Sample size. `0` = every row (no sampling)                        |
| `seed`      | int    | no        | `0`                   | RNG seed. Same seed + file = same sample                          |
| `table`     | string | no        | (no default)          | Specific table to read for multi-table sources                    |
| `unlimited` | bool   | no        | `false`               | Lift the 5,000,000-row file-loader cap so the sample sees every row |

## Response shape

Identical to [`read_table`](read_table.md): `{ schema, rows,
row_count, truncated, total_rows_available, cell_truncated }`. The
`rows` are the sampled rows, in original order.

## Notes

- For streaming formats the sample is drawn from the rows within the
  5 M-row cap; pass `unlimited: true` to sample from the whole file.
- A fixed `seed` makes repeated calls deterministic, which is handy for
  reproducible analysis.

## Example call

```json
{
  "name": "sample",
  "arguments": { "path": "/tmp/events.parquet", "limit": 100, "seed": 7 }
}
```

## See also

- [`read_table`](read_table.md) / [`tail`](tail.md).
- CLI [`octa --sample`](../../cli/sample.md).
