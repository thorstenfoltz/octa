# `drop_duplicates`

Remove duplicate rows from a table. The named columns form the duplicate
key; with no columns named, whole-row equality is used. Choose whether the
first or last occurrence of each key survives.

## When to use

- De-duplicating a file before a load or export.
- Collapsing rows that repeat a key (keep the newest/oldest per key).

## Input schema

| Parameter   | Type     | Required? | Default        | Description                                                          |
|-------------|----------|-----------|----------------|----------------------------------------------------------------------|
| `path`      | string   | no\*      | (no default)   | Path to the file (omit when `open_tab` is set)                       |
| `open_tab`  | string   | no        | (no default)   | Operate on an open GUI tab (`@active` or tab name)                   |
| `table`     | string   | no        | (no default)   | Specific table for multi-table sources                              |
| `on`        | string[] | no        | all columns    | Key columns; omit or pass `[]` for whole-row equality               |
| `keep`      | string   | no        | `first`        | `first` or `last`                                                    |
| `limit`     | integer  | no        | server default | Max rows to return. `0` = unlimited                                  |
| `unlimited` | bool     | no        | `false`        | Lift the 5,000,000-row file-loader cap so every row is read         |

\* `path` or `open_tab` is required.

Keys are compared on the cells' string representation, so `int(1)` and
`float(1.0)` are **not** treated as equal.

## Response shape

Returns the deduplicated table:

```json
{
  "schema": [ … ],
  "rows": [ [ … ], … ],
  "row_count": 95,
  "truncated": false,
  "total_rows_available": 95,
  "cell_truncated": false
}
```

## Example call

```json
{
  "name": "drop_duplicates",
  "arguments": {
    "path": "/data/contacts.csv",
    "on": ["email"],
    "keep": "last"
  }
}
```

## See also

- [`find_duplicates`](find_duplicates.md): report the duplicate rows
  instead of removing them.
