# `diff_tables`

Row-level diff of two files: return the rows present in only one side.
Where [`compare_schemas`](compare_schemas.md) diffs the column
*metadata*, `diff_tables` diffs the actual *rows*.

## When to use

To answer "what records changed between these two files / versions?"
without pulling both tables and comparing them yourself.

## How rows are compared

Each row is keyed by its **whole-row content** (every column, in order,
rendered to text). A row in A matches a row in B when their keys are
equal. Columns are therefore compared **positionally** - the two files
should share the same column order for the result to be meaningful.
Because matching is on rendered values, the diff works **across
formats** (a CSV row matches the equivalent Parquet row).

## Input schema

| Parameter   | Type   | Required? | Default               | Description                                           |
|-------------|--------|-----------|-----------------------|-------------------------------------------------------|
| `path_a`    | string | yes       | (no default)          | Path to the first file (side A)                       |
| `path_b`    | string | yes       | (no default)          | Path to the second file (side B)                      |
| `table_a`   | string | no        | (no default)          | Specific table to read from A (multi-table sources)   |
| `table_b`   | string | no        | (no default)          | Specific table to read from B (multi-table sources)   |
| `limit`     | int    | no        | server default (1000) | Max rows returned *per side*. `0` = unlimited         |
| `unlimited` | bool   | no        | `false`               | Lift the 5,000,000-row file-loader cap for both files |

## Response shape

```json
{
  "only_in_a": { "schema": [...], "rows": [...], "row_count": <n>, "truncated": <bool>, ... },
  "only_in_b": { "schema": [...], "rows": [...], "row_count": <n>, "truncated": <bool>, ... },
  "only_in_a_count": <n>,
  "only_in_b_count": <n>,
  "shared_keys": <n>
}
```

`only_in_a` / `only_in_b` are each a [`read_table`](read_table.md)-style
payload carrying the rows unique to that side (so `limit` and the
per-cell byte cap apply to each). `shared_keys` is the number of
distinct row keys present in both files. Rows present in both are not
returned.

## Example call

```json
{
  "name": "diff_tables",
  "arguments": { "path_a": "/tmp/users_v1.csv", "path_b": "/tmp/users_v2.csv" }
}
```

## See also

- [`compare_schemas`](compare_schemas.md): diff the column metadata
  instead of the rows.
- CLI [`octa --diff`](../../cli/diff.md).
