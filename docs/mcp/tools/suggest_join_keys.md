# `suggest_join_keys`

Rank the column pairs that would actually join two or more tables.

Use it before [`join_tables`](join_tables.md) when the key columns have
different names on each side, or are simply unknown. It looks at values,
not names, so `cust_id` and `id` pair up.

## Parameters

| Name        | Type            | Required | Default | Description                                        |
|-------------|-----------------|----------|---------|----------------------------------------------------|
| `paths`     | array of string | no       | `[]`    | Files to compare.                                  |
| `open_tabs` | array of string | no       | `[]`    | Open GUI tabs to include (tab name, or `@active`). |
| `sample`    | integer         | no       | `10000` | Rows sampled per table.                            |
| `limit`     | integer         | no       | `20`    | Maximum candidates returned.                       |

`paths` and `open_tabs` are combined; **two sources in total** are
required. One source is an error rather than an empty result, since an
empty list would read as "no keys found".

## Response

```json
{
  "candidates": [
    {
      "left_table": "orders.csv",
      "left_column": "cust_id",
      "right_table": "customers.csv",
      "right_column": "id",
      "overlap": 0.98,
      "left_distinct": 1.0,
      "right_distinct": 1.0,
      "score": 0.98
    }
  ],
  "sampled_rows_per_table": 10000
}
```

- `overlap`: share of the smaller distinct value set found in the larger.
- `left_distinct` / `right_distinct`: distinct values over sampled
  non-empty values, per side. A key is near `1.0`; a status column is
  near zero.
- `score`: `overlap` weighted by the better of the two distinctness
  figures. This is what stops a low-cardinality column that happens to
  overlap from outranking a real key. Candidates are returned best first.

## Notes

- **Read-only**, so it stays available under `--mcp-read-only`.
- Three or more sources produce every pairing between them.
- Sampled, so a high overlap is strong evidence rather than proof.
- Single columns only; composite keys are not suggested. Use
  [`unique_columns`](unique_columns.md) to find those.

## See also

- [`join_tables`](join_tables.md): perform the join once you know the keys.
- [`unique_columns`](unique_columns.md): which columns identify a row
  within one table.
- [Join Key Finder](../../usage/join-key-finder.md): the same feature in
  the GUI.
