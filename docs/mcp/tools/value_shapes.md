# `value_shapes`

What a column's values look like, with the specifics taken out: digits
become `9`, capital letters `A`, other letters `a`, punctuation stays
(`D-80331` is `A-99999`). The same shapes as the GUI's
[Value Shapes](../../usage/value-shapes.md) funnel switch and Quality
Report section, and `octa --shapes`.

Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name       | Type   | Meaning                                           |
|------------|--------|---------------------------------------------------|
| `path`     | string | The file. May be a cloud URL.                     |
| `open_tab` | string | Use an open GUI tab instead (name, or `@active`). |
| `table`    | string | Sheet or table name for multi-table sources.      |
| `column`   | string | The column to shape.                              |
| `limit`    | number | Maximum shapes to return. `0` for unlimited.      |

## Response

```json
{
  "column": "postcode",
  "empty": 2,
  "shape_count": 2,
  "shapes": [
    { "shape": "A-99999", "count": 118, "example": "D-10115" },
    { "shape": "A99999", "count": 3, "example": "D80331" }
  ]
}
```

A column where one shape dominates and a few values differ usually holds
typos or a second format. Values longer than 24 characters shorten runs
of four or more identical shape characters as `a(12)`.
