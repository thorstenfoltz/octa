# `fuzzy_join`

Join sources on how **similar** their values are rather than on exact
equality, for tables that name the same thing differently: the CRM says
`Mueller GmbH`, the sales sheet says `Mueller Gmbh.`.

Read-only, so it stays available under `--mcp-read-only`.

CLI mirror: [`octa --fuzzy-join`](../../cli/fuzzy-join.md).

## When to use

- [`join_tables`](join_tables.md) returned nothing, or far fewer rows
  than expected, and the key columns are names or addresses rather than
  identifiers.
- Reconciling two systems that were never given a shared key.

If you do not know which columns to compare, run
[`suggest_join_keys`](suggest_join_keys.md) first.

## Input schema

| Parameter   | Type     | Required? | Default      | Description                                                   |
|-------------|----------|-----------|--------------|---------------------------------------------------------------|
| `sources`   | object[] | yes       |              | Two or more `{path\|open_tab, table?}`, folded left to right. |
| `on`        | string[] | yes       |              | Column pairs `"LEFT=RIGHT"`. Several are averaged.            |
| `method`    | string   | no        | `edit_ratio` | `edit_ratio`, `jaro_winkler`, `token_set`.                    |
| `threshold` | number   | no        | `0.85`       | Minimum average score, `0.0` to `1.0`.                        |
| `block`     | string   | no        |              | `"LEFT=RIGHT"`; compare only rows agreeing exactly here.      |
| `how`       | string   | no        | `left`       | `inner`, `left`, `right`, `full`.                             |
| `max_rows`  | integer  | no        | `20000`      | Rows considered per side.                                     |
| `limit`     | integer  | no        | server cap   | Rows in the response. `0` for unlimited.                      |
| `unlimited` | boolean  | no        | `false`      | Lift the file-loader cap so every row is read.                |

## How matching works

Values are compared as normalised text: lowercased, whitespace runs
collapsed, punctuation removed. Each left row keeps its single best
partner at or above the threshold; ties go to the first occurrence.

The measures are the same ones
[`fuzzy_duplicates`](fuzzy_duplicates.md) uses within a single table, not
a second implementation.

## Response shape

The usual table payload (`schema`, `rows`, `row_count`, ...) plus a
`steps` array, one entry per join step:

```json
{
  "schema": [
    { "name": "customer", "type": "Utf8" },
    { "name": "account", "type": "Utf8" },
    { "name": "match_score_1", "type": "Float64" },
    { "name": "ambiguous_1", "type": "Boolean" }
  ],
  "rows": [["Mueller GmbH", "Mueller Gmbh.", 0.923, false]],
  "row_count": 1,
  "steps": [
    {
      "left_rows": 1,
      "right_rows": 1,
      "matched": 1,
      "ambiguous": 0,
      "capped": false
    }
  ]
}
```

`ambiguous_N` is true when the runner-up scored within 0.05 of the
winner. Those rows are the ones worth surfacing to the user: the engine
picked one, but it was close. `capped` says a side was cut to
`max_rows`, so the answer is partial.

## Blocking

Without `block` every left row is compared against every right row, which
is quadratic. A blocking column restricts comparison to rows agreeing on
it exactly, turning an impossible join into a quick one. It changes which
pairs are compared, never which of them match.

## Example call

```json
{
  "name": "fuzzy_join",
  "arguments": {
    "sources": [{ "path": "/data/crm.csv" }, { "path": "/data/sales.csv" }],
    "on": ["customer=account"],
    "threshold": 0.9,
    "block": "country=cc"
  }
}
```

## Ceilings

- Values are compared as text; no numeric or date tolerance.
- One partner per left row.
- With several sources the fold runs left to right, so a bad match early
  propagates; the score and flag are per step for exactly that reason.
- One set of settings applies to every step.

## See also

- [`octa --fuzzy-join`](../../cli/fuzzy-join.md), the CLI mirror.
- [`join_tables`](join_tables.md), the exact-equality join.
- [`suggest_join_keys`](suggest_join_keys.md), for choosing the columns.
- [`fuzzy_duplicates`](fuzzy_duplicates.md), the same measures within one
  table.
