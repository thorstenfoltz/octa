# `rfm_segments`

Customer segments (RFM): every customer marked 1 to 5 for recency (fewer days
since the last purchase is better), frequency (orders) and monetary (the
summed amount, exact), as quintiles where equal values share a mark, and the
segment name read off the marks. The same engine as the GUI's
[Customer segments (RFM)](../../usage/rfm.md) and `octa --rfm`.

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name       | Type   | Meaning                                                                                                       |
|------------|--------|---------------------------------------------------------------------------------------------------------------|
| `path`     | string | The file. May be a cloud URL.                                                                                 |
| `open_tab` | string | Use an open GUI tab instead (name, or `@active`).                                                             |
| `table`    | string | Sheet or table name for multi-table sources.                                                                  |
| `customer` | string | The column naming a customer.                                                                                 |
| `date`     | string | The date or timestamp of each purchase (ISO text works too).                                                  |
| `amount`   | string | What each row was worth. Empty counts as 0.                                                                   |
| `order`    | string | Rows with the same value are one order; a row without one is its own. Default: every row is one.              |
| `as_of`    | string | The day the segments are made for, e.g. `2024-12-31`; later purchases are left out. Default: the latest date. |
| `names`    | string | `seven` (default), `eleven` or `none`.                                                                        |
| `limit`    | number | At most this many customers in the answer; `0` for all.                                                       |

## Response shape

```json
{
  "as_of": "2024-03-30",
  "skipped": 0,
  "table": {
    "schema": [
      { "name": "customer", "type": "Utf8" },
      { "name": "last_date", "type": "Date32" },
      { "name": "days_since", "type": "Int64" },
      { "name": "orders", "type": "Int64" },
      { "name": "amount", "type": "Float64" },
      { "name": "recency", "type": "Int64" },
      { "name": "frequency", "type": "Int64" },
      { "name": "monetary", "type": "Int64" },
      { "name": "segment", "type": "Utf8" }
    ],
    "rows": [["bob", "2024-01-05", 85, 1, 100, 1, 1, 4, "Slipping away"]],
    "row_count": 1,
    "truncated": false,
    "total_rows_available": 1
  }
}
```

Rows without a customer or a readable date are left out and counted in
`skipped`. With `names: "none"` the `segment` column is left out.

## See also

- Command line: [`--rfm`](../../cli/rfm.md)
- In the app: [Customer segments (RFM)](../../usage/rfm.md)
