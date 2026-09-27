# `find_lookups`

Hidden lookup tables: columns that always have the same value for the same
key (a customer's name and city following `customer_id`), which means a flat
export really holds two tables. The same scan as the GUI's
[Find lookup tables](../../usage/lookup-tables.md) and `octa --lookups`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name              | Type   | Meaning                                                                |
|-------------------|--------|------------------------------------------------------------------------|
| `path`            | string | The file. May be a cloud URL.                                          |
| `open_tab`        | string | Use an open GUI tab instead (name, or `@active`).                      |
| `table`           | string | Sheet or table name for multi-table sources.                           |
| `min_consistency` | number | Share of rows (0 to 1) that must agree with their key. Default `0.95`. |

## Response

```json
{
  "findings": {
    "schema": [
      { "name": "key", "type": "Utf8" },
      { "name": "follows", "type": "Utf8" },
      { "name": "consistency_percent", "type": "Float64" },
      { "name": "conflicting_keys", "type": "Int64" },
      { "name": "breaking_rows", "type": "Int64" }
    ],
    "rows": [
      ["customer", "city", 100.0, 0, 0],
      ["customer", "name", 87.5, 1, 1]
    ],
    "row_count": 2,
    "truncated": false,
    "total_rows_available": 2
  }
}
```

Only keys that repeat are considered: a column with more than half as many
distinct values as rows is never a key. Breaking rows are often typos.
