# `generate_test_data`

Generate test data shaped like one or more real tables: the same columns,
numbers and dates drawn from the real spread, fake names, emails, IBANs and
IDs, code-like text in the same shape, empty cells at the real rate.
Several sources are generated together, so links between them still join.

Read-only, so it stays available under `--mcp-read-only`. To keep a table,
pass it on to [`write_table`](write_table.md).

## Parameters

| Name                | Type     | Meaning                                                                          |
|---------------------|----------|----------------------------------------------------------------------------------|
| `sources`           | object[] | One or more `{ path }` or `{ open_tab }`, each with an optional `table`.         |
| `rows`              | number   | Rows per generated table. Default: as many as the real table has.                |
| `seed`              | number   | The same seed gives the same rows. Default `0`.                                  |
| `rename_categories` | bool     | Replace the real values of small category columns with `value_1`, `value_2`, ... |
| `limit`             | number   | Maximum rows returned per table. `0` for unlimited.                              |

## Response

```json
{
  "plan": { "schema": [...], "rows": [["customers", "segment", "category", "0.25", "yes"]], "row_count": 9 },
  "tables": {
    "customers": { "schema": [...], "rows": [...], "row_count": 1000 },
    "orders": { "schema": [...], "rows": [...], "row_count": 1000 }
  }
}
```

`plan` has one row per column: `table`, `column`, `generator`,
`empty_share` and `real_values_kept` (`yes` only for a small category that
kept its real values).

See [Test Data](../../usage/test-data.md).
