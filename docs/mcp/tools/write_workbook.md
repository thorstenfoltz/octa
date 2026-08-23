# `write_workbook`

Write several tables into one `.xlsx`, one worksheet per entry.

A **write** tool: it is dropped when the server runs with `--mcp-read-only`,
and it refuses to run on a read-only assistant profile.

Use this instead of calling [`convert`](convert.md) once per table when the
user wants a single file. Five separate spreadsheets is not how anyone sends a
workbook to a colleague.

## Parameters

| Name       | Type     | Meaning                                    |
|------------|----------|--------------------------------------------|
| `out_path` | string   | Destination `.xlsx` path.                  |
| `sheets`   | object[] | One entry per worksheet, in workbook order |

Each entry in `sheets`:

| Name       | Type   | Meaning                                                 |
|------------|--------|---------------------------------------------------------|
| `path`     | string | Source file for this sheet. May be a cloud URL.         |
| `open_tab` | string | Use an open GUI tab instead (name, `@active`, or `#2`). |
| `table`    | string | Sheet or table name for a multi-table source file.      |
| `name`     | string | Worksheet name. Default: the file stem or the tab name. |

## Example

```json
{
  "out_path": "quarter.xlsx",
  "sheets": [
    { "path": "sales.csv", "name": "Sales" },
    { "path": "returns.parquet", "name": "Returns" },
    { "path": "stock.json" }
  ]
}
```

## Response

```json
{
  "path": "/home/you/quarter.xlsx",
  "sheets": ["Sales", "Returns", "stock"],
  "rows": [1200, 87, 340]
}
```

`sheets` is what the workbook actually contains, which may differ from what was
asked for: sheet names are corrected to Excel's rules before writing, at most 31
characters, no forbidden punctuation, and duplicates numbered `Report`,
`Report_2`. A write therefore cannot produce a workbook Excel refuses to open.

## Notes

- **Every source is read before anything is written.** A workbook half-written
  because the fifth source was unreadable is worse than no workbook at all.
- Multi-table sources contribute their named `table`, or their first table if
  `table` is omitted.
- The GUI equivalent is **File > Export workbook...** and the CLI equivalent is
  `octa --to-workbook OUT.xlsx FILE...`; see [Saving](../../usage/saving.md).
