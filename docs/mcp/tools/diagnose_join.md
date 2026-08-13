# `diagnose_join`

Explain why a join between two key columns returns fewer rows than
expected.

Use it after [`suggest_join_keys`](suggest_join_keys.md) has named the
columns and before [`join_tables`](join_tables.md) actually joins them,
whenever the row count comes back suspiciously low.

## Parameters

| Name           | Type    | Required | Default | Description                                       |
|----------------|---------|----------|---------|---------------------------------------------------|
| `path`         | string  | yes\*    | -       | Left-hand file (or cloud URL).                    |
| `open_tab_a`   | string  | no       | -       | Left-hand open GUI tab (tab name, or `@active`).  |
| `left_column`  | string  | yes      | -       | Key column on the left, by name.                  |
| `path_b`       | string  | yes\*    | -       | Right-hand file (or cloud URL).                   |
| `open_tab_b`   | string  | no       | -       | Right-hand open GUI tab (tab name, or `@active`). |
| `right_column` | string  | yes      | -       | Key column on the right, by name.                 |
| `sample`       | integer | no       | `10000` | Rows sampled per side.                            |

\* Each side needs either its `path` or its `open_tab_*`.

A column name that does not exist is an error listing the names that do,
rather than a silent fallback that would produce a confident, meaningless
diagnosis.

## Response

```json
{
  "left_rows": 5000,
  "right_rows": 4800,
  "distinct_left": 4900,
  "distinct_right": 4700,
  "matched_left": 120,
  "matched_right": 118,
  "unmatched_left": 4780,
  "unmatched_right": 4582,
  "fixes": [
    { "kind": "trim_whitespace", "would_match": 4650 }
  ],
  "capped": false,
  "sampled_rows_per_side": 10000
}
```

- Counts are over **distinct keys**, not rows.
- `fixes` names the single normalisation that would raise the match
  count: `trim_whitespace`, `ignore_case`, `collapse_whitespace`,
  `strip_punctuation` or `strip_leading_zeros`. Each is scored by
  recomputing the same containment count through that normalisation.
- A fix appears only when it **strictly** beats the current count, so an
  empty `fixes` list is a real answer: no simple change helps, and the
  two columns probably hold genuinely different things.
- `capped` is true when either side was longer than `sample`, in which
  case the counts are partial.

## Notes

- **Read-only**, so it stays available under `--mcp-read-only`. It
  diagnoses; it changes neither table.
- Nothing is applied for you. Fix the data with
  [`transform_columns`](transform_columns.md), or at the source, and run
  the diagnosis again.

## See also

- [`suggest_join_keys`](suggest_join_keys.md): which columns to join on.
- [`join_tables`](join_tables.md): perform the join.
- [`fuzzy_join`](fuzzy_join.md): join on similarity when no
  normalisation makes the keys equal.
- [Join Diagnostics](../../usage/join-diagnostics.md): the same feature
  in the GUI.
