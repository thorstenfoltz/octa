# `diff_tables`

Compare two files and return what differs. Where
[`compare_schemas`](compare_schemas.md) diffs the column *metadata*,
`diff_tables` diffs the actual *rows*. The `mode` parameter trades off
how rows are matched, from coarse whole-row membership to precise
cell-level change detection.

## When to use

To answer "what records changed between these two files / versions?"
without pulling both tables and comparing them yourself. Use `ordered`
or `join` when you need to know *which cells* changed, not just which
whole rows are unique to a side.

## Modes

- **`set`** (default): each row is keyed by its **whole-row content**
  (every column, in order, rendered to text). Columns are compared
  **positionally**, so the two files should share the same column
  order. Because matching is on rendered values, it works **across
  formats** (a CSV row matches the equivalent Parquet row). Returns the
  rows unique to each side.
- **`ordered`**: lines up row *i* of A with row *i* of B and compares
  cell by cell over the shared columns. Reports matched rows that
  differ (with the differing column names) plus trailing rows unique to
  the longer side.
- **`join`**: matches rows on the `on` key column(s) (matched by
  **name**), then reports keys added (in B only), removed (in A only),
  and changed (matched keys whose non-key cells differ, with the
  differing column names).

## Input schema

| Parameter   | Type     | Required?       | Default               | Description                                           |
|-------------|----------|-----------------|-----------------------|-------------------------------------------------------|
| `path_a`    | string   | yes             | (no default)          | Path to the first file (side A)                       |
| `path_b`    | string   | yes             | (no default)          | Path to the second file (side B)                      |
| `mode`      | string   | no              | `set`                 | `set`, `ordered`, or `join`                           |
| `on`        | string[] | for `join` only | (no default)          | Key column(s) for `join`, matched by name             |
| `table_a`   | string   | no              | (no default)          | Specific table to read from A (multi-table sources)   |
| `table_b`   | string   | no              | (no default)          | Specific table to read from B (multi-table sources)   |
| `limit`     | int      | no              | server default (1000) | Max rows returned *per side*. `0` = unlimited         |
| `unlimited` | bool     | no              | `false`               | Lift the 5,000,000-row file-loader cap for both files |

## Response shape

For `mode: "set"`:

```json
{
  "mode": "set",
  "only_in_a": { "schema": [...], "rows": [...], "row_count": <n>, "truncated": <bool>, ... },
  "only_in_b": { "schema": [...], "rows": [...], "row_count": <n>, "truncated": <bool>, ... },
  "only_in_a_count": <n>,
  "only_in_b_count": <n>,
  "shared_keys": <n>
}
```

For `mode: "ordered"` / `"join"` the response additionally carries the
changed rows:

```json
{
  "mode": "join",
  "only_in_a": { ... }, "only_in_b": { ... },
  "changed_a": { ... }, "changed_b": { ... },
  "changed": [ { "row_a": <i>, "row_b": <j>, "changed_columns": ["name", ...] }, ... ],
  "only_in_a_count": <n>, "only_in_b_count": <n>,
  "changed_count": <n>, "unchanged_count": <n>
}
```

`only_in_a` / `only_in_b` (and `changed_a` / `changed_b`) are each a
[`read_table`](read_table.md)-style payload (so `limit` and the
per-cell byte cap apply to each). `changed_a[k]` and `changed_b[k]`
line up with `changed[k]`, which names the differing columns for that
pair. For `set`, `shared_keys` is the number of distinct row keys
present in both files. Unchanged rows are not returned.

## Example call

```json
{
  "name": "diff_tables",
  "arguments": {
    "path_a": "/tmp/users_v1.csv",
    "path_b": "/tmp/users_v2.csv",
    "mode": "join",
    "on": ["id"]
  }
}
```

## See also

- [`compare_schemas`](compare_schemas.md): diff the column metadata
  instead of the rows.
- CLI [`octa --diff`](../../cli/diff.md).
