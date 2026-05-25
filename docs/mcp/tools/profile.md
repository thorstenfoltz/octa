# `profile`

Return **per-column statistics** for a tabular file: data type, min,
max, approximate distinct count, mean, standard deviation, quartiles,
row count, and null percentage. The fastest way to understand an
unfamiliar dataset before reading rows or writing SQL.

Internally the file is registered as the DuckDB temp table `data` and
`SUMMARIZE data` is run. Column types are preserved on registration, so
numeric columns get real numeric statistics rather than lexical ones.

## When to use

- A first pass on an unknown dataset: ranges, null density, cardinality.
- Spotting data-quality issues (unexpected nulls, suspicious min/max).
- Deciding which columns are worth a closer `read_table` or `run_sql`.

## Input schema

| Parameter   | Type   | Required? | Default      | Description                                                        |
|-------------|--------|-----------|--------------|--------------------------------------------------------------------|
| `path`      | string | yes       | (no default) | Path to the file                                                   |
| `table`     | string | no        | (no default) | Specific table for multi-table sources                             |
| `unlimited` | bool   | no        | `false`      | Lift the 5,000,000-row file-loader cap so SUMMARIZE sees every row |

## Response shape

```json
{
  "column_count": 3,
  "columns": [
    {
      "column_name": "id",
      "column_type": "BIGINT",
      "min": "1",
      "max": "10000",
      "approx_unique": 10000,
      "avg": "5000.5",
      "std": "2886.9",
      "q25": "2500",
      "q50": "5000",
      "q75": "7500",
      "count": 10000,
      "null_percentage": "0.00"
    }
  ]
}
```

Each object is one source column; the keys are DuckDB's `SUMMARIZE`
output columns. Exact keys can vary slightly with DuckDB versions.

## Example call

```json
{
  "name": "profile",
  "arguments": { "path": "/tmp/sales.parquet" }
}
```

## Notes

- For streaming formats (Parquet, CSV, TSV) the statistics are
  computed over the server's initial-load row cap (5 Million rows by
  default), not necessarily the entire file. Pass
  `unlimited: true` to profile every row.
- Columns a reader leaves as `Utf8` are summarised lexically (string
  min/max, no mean). Cast inside [`run_sql`](run_sql.md) if you need
  numeric stats on a string-typed column.

## See also

- [`schema`](schema.md) — column names + types only, no statistics.
- [`value_frequency`](value_frequency.md) — the value distribution of a
  single column.
- [`run_sql`](run_sql.md) — run `SUMMARIZE data` or custom aggregates
  yourself.
