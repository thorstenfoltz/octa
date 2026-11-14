# `number_bands`

Numbers into labelled bands (`18-34`, `65+`, `<18`), or labelled bands back
into numbers. The same engine as the GUI's
[Number bands](../../usage/number-bands.md) and `octa --bands`. Changes no
file.

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name       | Type            | Meaning                                                |
|------------|-----------------|--------------------------------------------------------|
| `path`     | string          | The file. May be a cloud URL.                          |
| `open_tab` | string          | Use an open GUI tab instead (name, or `@active`).      |
| `table`    | string          | Sheet or table name for multi-table sources.           |
| `column`   | string          | The column to band, or to read back.                   |
| `by`       | string          | `width` (default), `count` or `edges`.                 |
| `count`    | number          | How many bands for `width` and `count`. Default `5`.   |
| `edges`    | array of number | Where bands start, for `edges`.                        |
| `back`     | string          | Read bands back instead: `middle`, `lower` or `upper`. |
| `limit`    | number          | At most this many rows in the answer; `0` for all.     |

## Response shape

For a `group` column holding `unter 18`, `18 bis 24` and `65 und älter`, with
`back: "middle"`:

```json
{
  "column": "group_middle",
  "unread": 0,
  "table": {
    "schema": [
      { "name": "group", "type": "Utf8" },
      { "name": "group_middle", "type": "Float64" }
    ],
    "rows": [["unter 18", 15.0], ["18 bis 24", 21.0], ["65 und älter", 68.0]],
    "row_count": 3,
    "truncated": false,
    "total_rows_available": 3
  }
}
```

Only the source column and the new one come back. `unread` counts labels with
text but no number in them; those cells are null. An open band takes the
width of the band next to it.

## See also

- Command line: [`--bands`](../../cli/bands.md)
- In the app: [Number bands](../../usage/number-bands.md)
