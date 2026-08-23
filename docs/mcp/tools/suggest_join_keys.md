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
      "score": 0.98,
      "left_orphans": 2,
      "left_distinct_values": 100,
      "right_orphans": 0,
      "right_distinct_values": 98
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
- `left_orphans` out of `left_distinct_values`, and `right_orphans` out
  of `right_distinct_values`: how many distinct values on each side find
  no partner on the other. These are what separate two candidates that
  score identically, which happens whenever both tables number their rows
  from 1: with 4 customers and 1,000 orders, both
  `customers.id -> orders.id` and `customers.id -> orders.customer_id`
  score a perfect 1.00 and report `left_orphans: 0`, while the first
  reports `right_orphans: 996` and the second `0`.

  **Only one of the two directions settles such a tie**, the one read
  from the child table's side, and nothing labels which side that is - so
  both are always returned and you read it off the pair. The source order
  does not matter.

## How the score is calculated

Each column becomes the set of its distinct values, read from the first
`sample` rows. Cells are turned into text and trimmed, and empty cells
are skipped entirely, counting towards neither the distinct set nor the
denominator. Then, per column pair:

```text
shared       = how many values appear in BOTH distinct sets
overlap      = shared / the smaller of the two distinct counts
distinctness = distinct values / non-empty values sampled   (each side)

score        = overlap x the LARGER of the two distinctness values

orphans      = distinct values on the left - shared
```

Taking the **larger** distinctness is deliberate: a foreign key is unique
on the parent side and repeats on the child side, so the child's figure is
low by design and taking the smaller would penalise the very shape being
looked for. A pair sharing no value is not a candidate, and a pair scoring
below `0.2` is dropped as noise.

`overlap` and `score` are symmetric; the orphan counts are not, which is
why both directions are returned. "Left" is the earlier source in the
request, not a claim about which table is the child.

[Join Key Finder](../../usage/join-key-finder.md) works the same
arithmetic through with a checkable example.

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
