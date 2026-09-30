# `unique_columns`

Find columns (and optional small combinations) whose values are
unique across a tabular file. Useful for primary-key reconnaissance
on undocumented databases or freshly imported CSVs.

CLI mirror: [`octa --unique-columns`](../../cli/unique-columns.md).

## When to use

- "What's the primary key here?" on a CSV / Parquet with no
  declared key.
- Spotting a candidate join column between two tables.
- Quickly checking that an `id` column really is unique before
  trusting it.

## Input schema

| Parameter        | Type    | Required? | Default      | Description                                               |
|------------------|---------|-----------|--------------|-----------------------------------------------------------|
| `path`           | string  | yes       | (no default) | Path to the file.                                         |
| `table`          | string  | no        | (no default) | Specific table for multi-table sources.                   |
| `max_combo_size` | integer | no        | `1`          | Max combo size (clamped to `[1, 3]`). `1` = singles only. |
| `unlimited`      | boolean | no        | `false`      | Lift the 2,000,000-row file-loader cap.                   |

## Uniqueness rule

A column is `is_unique` only when:

1. `distinct_count == total_rows`, AND
2. `null_count == 0`, AND
3. `total_rows > 0`.

Rule 2 is deliberate: most databases reject `NULL` in a primary key,
so a column with a single null and otherwise distinct values is not
considered a PK candidate.

## Combo strategy

When `max_combo_size > 1`, the tool tests pairs (and triples, if
`max_combo_size = 3`). To avoid pointless work, only columns whose
own `distinct_count` is in `(1, total_rows)` are combined. Already
unique columns are skipped (they'd trivially make any combo unique).

## Response shape

```json
{
  "total_rows": 10000,
  "single": [
    {
      "column": "id",
      "distinct_count": 10000,
      "null_count": 0,
      "is_unique": true
    },
    {
      "column": "region",
      "distinct_count": 5,
      "null_count": 0,
      "is_unique": false
    }
  ],
  "combos": [
    {
      "columns": ["first_name", "last_name"],
      "distinct_count": 9876,
      "is_unique": false
    }
  ]
}
```

## Example call

Singles only:

```json
{
  "name": "unique_columns",
  "arguments": {
    "path": "/data/users.csv"
  }
}
```

Test pairs too:

```json
{
  "name": "unique_columns",
  "arguments": {
    "path": "/data/orders.parquet",
    "max_combo_size": 2
  }
}
```

Scan the full file:

```json
{
  "name": "unique_columns",
  "arguments": {
    "path": "/data/huge.parquet",
    "unlimited": true
  }
}
```

## Performance notes

Octa first checks whether any single column is unique. This is fast: the effort grows only in proportion to the size of the table (rows × columns).
If no single column is unique, the tool tests combinations of two or three columns. The number of these adds up quickly, with 20 candidate columns there are about 190 possible pairs and over 1,000 triples,
and each one is checked again across all rows. So for wide tables, this step can take considerably longer.
For that reason, the maximum combination size is fixed at 3 (max_combo_size). Larger combinations aren't tried, because their number would otherwise grow explosively.

## See also

- [`octa --unique-columns`](../../cli/unique-columns.md), the CLI mirror.
- [`find_duplicates`](find_duplicates.md): the symmetric tool for
  spotting rows that *aren't* unique under a chosen key.
- [`profile`](profile.md): broader per-column statistics.
