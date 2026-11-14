# `quality_report`

What is missing, odd or inconsistent in a table, column by column, with an
overall score. The same report as the GUI's
[Data Quality Report](../../usage/data-quality-report.md).

Read-only, so it stays available under `--mcp-read-only`.

## Input schema

| Name        | Type    | Meaning                                                   |
|-------------|---------|-----------------------------------------------------------|
| `path`      | string  | The file. May be a cloud URL.                             |
| `open_tab`  | string  | Use an open GUI tab instead (name, or `@active`).         |
| `table`     | string  | Sheet or table name for multi-table sources.              |
| `unlimited` | boolean | Lift the initial-load row cap on a file. Default `false`. |

## Response shape

```json
{
  "overall_score": 94.0,
  "columns": {
    "schema": [
      { "name": "column_name", "type": "Utf8" },
      { "name": "data_type", "type": "Utf8" },
      { "name": "null_percentage", "type": "Int64" },
      ...
    ],
    "rows": [
      ["customer", "Utf8", 0, 0.6, 0, "no", "", 1, "not tested: not numbers",
       "not tested: not dates", "not tested: not numbers or IDs", "consistent", 92]
    ],
    "row_count": 3,
    "truncated": false,
    "total_rows_available": 3,
    "cell_truncated": false
  },
  "sections": [
    { "title": "<tab name>", "table": { "schema": [...], "rows": [...], ... } }
  ]
}
```

`columns` holds one row per column of the table, and is never cut short by the row
limit. Its columns are:

| Column                                                                   | Meaning                                                |
|--------------------------------------------------------------------------|--------------------------------------------------------|
| `column_name`, `data_type`                                               | The column and its type.                               |
| `null_percentage`, `distinct_ratio`                                      | How much is missing, and how much of the rest differs. |
| `outlier_count`                                                          | Values the report treats as outliers.                  |
| `pii_flag`, `pii_kind`                                                   | Whether it looks like personal data, and what kind.    |
| `type_consistency`                                                       | How well the values fit one type.                      |
| `benford_verdict`, `calendar_verdict`, `number_verdict`, `shape_verdict` | Each check's verdict, in the report's own words.       |
| `score`                                                                  | The column's mark out of 100.                          |

`sections` holds the findings that span several columns, one entry per extra tab
of the report, and is empty when there are none. A section's `title` is English
on an MCP server and follows Octa's language inside the assistant, so match on its
table's columns, not on the title. See
[Data Quality Report](../../usage/data-quality-report.md) for what each measure
and verdict means.

On a database tab in the assistant that holds only part of its table, the counts
run on the server, the response has `"on_server": true`, and `from_loaded_rows`
names the parts that looked at the loaded rows instead. See
[Analyses on Live Databases](../../usage/analyses-on-live-databases.md).

## See also

- In the app: [Data Quality Report](../../usage/data-quality-report.md)
