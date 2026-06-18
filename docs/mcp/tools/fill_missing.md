# `fill_missing`

Fill the empty/null cells of one column using a strategy. Existing values
are left alone.

## When to use

- Imputing gaps before analysis or a downstream load.
- Carrying a value forward/backward to un-merge grouped exports.

## Input schema

| Parameter   | Type     | Required? | Default        | Description                                                              |
|-------------|----------|-----------|----------------|--------------------------------------------------------------------------|
| `path`      | string   | no\*      | (no default)   | Path to the file (omit when `open_tab` is set)                           |
| `open_tab`  | string   | no        | (no default)   | Operate on an open GUI tab (`@active` or tab name)                       |
| `table`     | string   | no        | (no default)   | Specific table for multi-table sources                                  |
| `column`    | string   | yes       | (no default)   | Column whose missing cells to fill                                       |
| `strategy`  | string   | yes       | (no default)   | `mean`, `median`, `mode`, `ffill`, `bfill`, or `const`                   |
| `value`     | string   | no        | (no default)   | Fill value for `const` (ignored otherwise)                              |
| `limit`     | integer  | no        | server default | Max rows to return. `0` = unlimited                                      |
| `unlimited` | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so every row is read             |

\* `path` or `open_tab` is required. `mean`/`median` require a numeric column.

## Response shape

Returns the table with the column filled:

```json
{
  "schema": [ … ],
  "rows": [ [ … ], … ],
  "row_count": 100,
  "truncated": false,
  "total_rows_available": 100,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "fill_missing",
  "arguments": {
    "path": "/data/readings.csv",
    "column": "temperature",
    "strategy": "median"
  }
}
```

## See also

- [`transform_columns`](transform_columns.md): rename/cast/drop columns.
