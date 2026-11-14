# `funnel`

Count how many people (`who`) get through `steps` in that order, and where
they drop out. A person reaches a step when they have that event at or after
the moment they reached the step before. The same engine as the GUI's
[Funnel](../../usage/funnel.md) and `octa --funnel`.

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name          | Type     | Meaning                                                                        |
|---------------|----------|--------------------------------------------------------------------------------|
| `path`        | string   | The file. May be a cloud URL.                                                  |
| `open_tab`    | string   | Use an open GUI tab instead (name, or `@active`).                              |
| `table`       | string   | Sheet or table name for multi-table sources.                                   |
| `who`         | string   | The column naming a person or customer.                                        |
| `event`       | string   | The column saying what happened in each row.                                   |
| `when`        | string   | The date or timestamp column (ISO text works). Omit to use the row order.      |
| `steps`       | string[] | Event values in funnel order, two or more, each once.                          |
| `within_days` | number   | Count a later step only within this many days of the first step. Needs `when`. |

## Response shape

For `events.csv` with `who: "user"`, `event: "event"`, `when: "at"`,
`steps: ["view", "cart", "pay"]`:

```json
{
  "skipped": 0,
  "table": {
    "schema": [
      { "name": "step", "type": "Utf8" },
      { "name": "users", "type": "Int64" },
      { "name": "share_of_first", "type": "Float64" },
      { "name": "share_of_previous", "type": "Float64" },
      { "name": "dropped", "type": "Int64" }
    ],
    "rows": [
      ["view", 3, 100.0, 100.0, 0],
      ["cart", 2, 66.7, 66.7, 1],
      ["pay", 1, 33.3, 50.0, 1]
    ],
    "row_count": 3,
    "truncated": false,
    "total_rows_available": 3
  }
}
```

The shares are percent. `skipped` counts the rows with no `who` or no
readable time.

## See also

- In the app: [Funnel](../../usage/funnel.md)
- Command line: [`--funnel`](../../cli/funnel.md)
