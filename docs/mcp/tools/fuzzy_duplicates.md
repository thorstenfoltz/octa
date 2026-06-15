# `fuzzy_duplicates`

Find **near-duplicate rows** in a tabular file: rows that are *almost* the same
on the chosen columns (typos, spacing, reordered words), grouped into clusters
with a similarity score. The fuzzy counterpart of
[`find_duplicates`](find_duplicates.md).

## When to use

- Entity resolution: "Jon Smith" vs "John Smith", "ACME Inc" vs "ACME, Inc.".
- Data-quality review before a merge or load, where exact dedup misses typos.
- Producing a cluster report for a human to confirm and clean.

## Input schema

| Parameter      | Type     | Required? | Default       | Description                                                                          |
|----------------|----------|-----------|---------------|--------------------------------------------------------------------------------------|
| `path`         | string   | yes*      | (no default)  | Path to the file (omit when `open_tab` is set)                                       |
| `open_tab`     | string   | no        | (no default)  | Operate on an open GUI tab (`@active` or a tab name) instead of a file              |
| `table`        | string   | no        | (no default)  | Specific table for multi-table sources                                              |
| `key_columns`  | string[] | yes       | (no default)  | Columns compared (scores are averaged across them)                                  |
| `method`       | string   | no        | `edit_ratio`  | `edit_ratio` (typos), `jaro_winkler` (names), or `token_set` (word order)           |
| `threshold`    | number   | no        | `0.85`        | Match threshold, `0.0`..=`1.0`                                                       |
| `lower`        | bool     | no        | `true`        | Lowercase before comparing                                                           |
| `collapse_ws`  | bool     | no        | `true`        | Collapse whitespace before comparing                                                |
| `strip_punct`  | bool     | no        | `true`        | Strip punctuation before comparing                                                  |
| `block_column` | string   | no        | (no default)  | Only compare rows sharing this column's exact value (makes large tables feasible)   |
| `max_rows`     | integer  | no        | `20000`       | Cap on rows scanned                                                                  |
| `unlimited`    | bool     | no        | `false`       | Lift the 5,000,000-row file-loader cap so every row is loaded from disk             |

This is **read-only** analytics: it returns clusters and writes nothing, so it
stays available under `--mcp-read-only`.

## Response shape

```json
{
  "cluster_count": 2,
  "clusters": [
    { "cluster": 1, "rows": [0, 4], "score": 0.91 },
    { "cluster": 2, "rows": [7, 9, 12], "score": 0.86 }
  ],
  "compared_rows": 5000,
  "capped": false
}
```

`rows` are zero-based row indices into the scanned table. `score` is the
**lowest** linking similarity inside the cluster (the honest worst case).
`capped` is `true` when the table held more than `max_rows` rows.

## Example call

```json
{
  "name": "fuzzy_duplicates",
  "arguments": {
    "path": "/tmp/companies.csv",
    "key_columns": ["company_name"],
    "method": "token_set",
    "threshold": 0.8,
    "block_column": "country"
  }
}
```

## See also

- [`find_duplicates`](find_duplicates.md): exact key-based duplicates.
- [`run_sql`](run_sql.md): custom matching logic.
