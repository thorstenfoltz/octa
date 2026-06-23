# `join_tables`

Match rows between two or more sources on one or more shared key columns,
like a SQL JOIN or a spreadsheet VLOOKUP. Runs through DuckDB.

## When to use

- Enriching one table with columns from another (orders + customers).
- Checking which keys exist in both tables (inner) or only one (left/right).

## Input schema

| Parameter   | Type     | Required? | Default        | Description                                                                                                 |
|-------------|----------|-----------|----------------|-------------------------------------------------------------------------------------------------------------|
| `sources`   | object[] | yes       | (no default)   | Two or more sources. Each has `path` (file) **or** `open_tab` (`@active` / tab name), plus optional `table` |
| `on`        | string[] | yes       | (no default)   | Key column name(s); must exist in every source. At least one required                                       |
| `how`       | string   | no        | `left`         | `left`, `inner`, `right`, or `full`                                                                         |
| `limit`     | integer  | no        | server default | Max rows to return. `0` = unlimited                                                                         |
| `unlimited` | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so every source row is read                                          |

## Response shape

Returns the joined table:

```json
{
  "schema": [ … ],
  "rows": [ [ … ], … ],
  "row_count": 80,
  "truncated": false,
  "total_rows_available": 80,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "join_tables",
  "arguments": {
    "sources": [
      { "path": "/data/orders.parquet" },
      { "path": "/data/customers.parquet" }
    ],
    "on": ["customer_id"],
    "how": "left"
  }
}
```

## See also

- [`union_tables`](union_tables.md): stack tables vertically instead of
  matching on a key.
- [`run_sql`](run_sql.md): full control over join conditions and output
  columns.
