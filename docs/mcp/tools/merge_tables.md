# `merge_tables`

Merge two or more versions of a table, per row and per cell. With an
`original` (the table they were all edited from) a change made in one version
is taken and only different changes to one cell, or a row deleted in one
version and edited in another, conflict. Without it, every cell where the
versions differ conflicts and rows from any version are kept.

Read-only, so it stays available under `--mcp-read-only`. To save the result,
pass `merged` on to [`write_table`](write_table.md).

## Parameters

| Name        | Type     | Meaning                                                                    |
|-------------|----------|----------------------------------------------------------------------------|
| `versions`  | array    | Two or more `{ path }` or `{ open_tab }` objects. Paths may be cloud URLs. |
| `original`  | object   | `{ path }` or `{ open_tab }`: what every version was edited from.          |
| `keys`      | string[] | Column(s) that identify a row. Empty matches rows by position.             |
| `prefer`    | number   | Settle every conflict in favour of this version (1 = the first).           |
| `limit`     | number   | Maximum merged rows to return. 0 for unlimited.                            |
| `unlimited` | bool     | Lift the streaming row cap for this call.                                  |

Use [`suggest_join_keys`](suggest_join_keys.md) when you do not know the key.

## Response

```json
{
  "conflict_count": 1,
  "conflicts": { "schema": [...], "rows": [["1", "v", "x", "a", "b", "x"]], "row_count": 1 },
  "status_counts": { "unchanged": 40, "changed": 5, "added": 1, "conflict": 1 }
}
```

`conflicts` lists what is still open, one row per decision: `row` (1-based,
in the merged table), `column`, `original`, then `version_1`, `version_2`, ...
with what each version holds. A row deleted in some versions and edited in
another has no column and shows `(deleted)` / `(edited)`.

`merged` is only present once `conflict_count` is 0. It is a table payload
like every other row-returning tool.

See [Merge Versions](../../usage/merge-versions.md).
