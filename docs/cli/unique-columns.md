# `octa --unique-columns`

Find columns, and optional small combinations, whose values are
unique across a file. Useful for spotting primary-key candidates in
undocumented data.

## Synopsis

```bash
octa --unique-columns FILE [--table NAME] [--max-combo N] [-f FORMAT]
```

| Flag                    | Required | Meaning                                           |
|-------------------------|----------|---------------------------------------------------|
| `--unique-columns FILE` | yes      | The file to scan.                                 |
| `--table NAME`          | no       | Specific table for multi-table sources.           |
| `--max-combo N`         | no       | Max combo size (default 1; clamped to `[1, 3]`).  |
| `-f`, `--format FORMAT` | no       | Output format: `tsv` (default), `json`, or `csv`. |

## Uniqueness rule

A column is `is_unique` only when:

1. `distinct_count == total_rows`, AND
2. `null_count == 0`, AND
3. `total_rows > 0`.

Rule 2 is deliberate: most databases reject `NULL` in a primary key,
so a column with a single null and otherwise distinct values is not
a PK candidate.

## Combo strategy

When `--max-combo > 1`, the tool tests pairs (and triples if the
value is 3). To avoid pointless work, only columns whose own
`distinct_count` is in `(1, total_rows)` are combined. Already-unique
columns are skipped, since they'd make any combo trivially unique too.

## Output

A five-column table:

| Column           | Meaning                                               |
|------------------|-------------------------------------------------------|
| `scope`          | `single` for one column, `combo` for a pair / triple. |
| `columns`        | Column name (singles) or `+`-joined names (combos).   |
| `distinct_count` | Number of distinct keys observed.                     |
| `null_count`     | Single columns only; empty for combos.                |
| `is_unique`      | Boolean.                                              |

## Examples

### Single columns

```bash
$ octa --unique-columns users.csv
scope   columns       distinct_count  null_count  is_unique
single  id            10000           0           true
single  email         9998            2           false
single  region        5               0           false
```

`email` has 9998 distinct values across 10000 rows AND two nulls, so it is
not a PK candidate.

### Test pairs too

```bash
$ octa --unique-columns orders.parquet --max-combo 2
scope   columns                distinct_count  null_count  is_unique
single  user_id                4231            0           false
single  ordered_at             9874            0           false
single  payment_method         3               0           false
combo   user_id + ordered_at   10000                       true
```

The `(user_id, ordered_at)` pair is unique → that's the composite key.

### JSON for downstream tooling

```bash
octa --unique-columns data.csv -f json | jq '.[] | select(.is_unique)'
```

## Performance notes

`--max-combo 3` can be slow on wide tables (`C(50, 3) ≈ 19,600`
combinations to test). Start with `--max-combo 2` and only escalate
if no pair turns out to be unique.

## See also

- [MCP `unique_columns`](../mcp/tools/unique_columns.md): same
  feature over MCP.
- [`octa --sql`](sql.md): `SELECT COUNT(*), COUNT(DISTINCT col) FROM
  data` is the manual equivalent for one column.
- In the app: [Join Key Finder](../usage/join-key-finder.md)
- MCP: [`suggest_join_keys`](../mcp/tools/suggest_join_keys.md)
