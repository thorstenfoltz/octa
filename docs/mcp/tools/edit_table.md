# `edit_table`

Edit an existing tabular file in place: set individual cells, insert rows,
and delete rows, then save back through the file's native writer.

## When to use

- "Fix the value in row 3 of the `status` column."
- "Add these two rows to the table."
- "Delete rows 10 and 11."

For writing a brand-new file from inline data, use
[`write_table`](write_table.md). For schema changes (rename / add / drop a
column), use a dedicated tool: `edit_table` only mutates cell values and
row counts.

## Input schema

| Parameter      | Type   | Required? | Default      | Description                                                              |
|----------------|--------|-----------|--------------|--------------------------------------------------------------------------|
| `path`         | string | yes       | (no default) | File to edit, in place                                                   |
| `table`        | string | no        | (no default) | For multi-table sources (SQLite, DuckDB, GeoPackage, Excel), which table |
| `set`          | array  | no        | `[]`         | Cell edits: `{ "row": int, "col": int \| string, "value": any }`         |
| `insert_rows`  | array  | no        | `[]`         | Rows to insert: `{ "at"?: int, "values": [...] }`                        |
| `delete_rows`  | array  | no        | `[]`         | 0-based row indices to delete                                            |
| `unlimited`    | bool   | no        | `false`      | Load the whole file before editing (and rewrite it in full for non-DB formats) |

- `set[].col` is either a 0-based column **index** or a column **name**.
- `set[].row` and `delete_rows[]` are 0-based indices into the loaded rows.
- `insert_rows[].at` is the 0-based insertion index; omit it to append at
  the end. `values` line up positionally with the columns.
- Cell values are coerced to the target column's type, the same way
  [`write_table`](write_table.md) coerces them.

Operations are applied in a fixed order: deletes first (highest index
first, so lower indices stay valid), then inserts, then cell edits against
the resulting layout.

## Response shape

```json
{
  "cells_set": <n>,
  "rows_inserted": <n>,
  "rows_deleted": <n>,
  "path": "<path>"
}
```

## Example calls

### Set a cell, insert a row, delete a row

```json
{
  "name": "edit_table",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "people",
    "set": [{ "row": 0, "col": "name", "value": "ALICE" }],
    "insert_rows": [{ "values": [4, "dave"] }],
    "delete_rows": [2]
  }
}
```

Response:

```json
{ "cells_set": 1, "rows_inserted": 1, "rows_deleted": 1, "path": "/data/app.sqlite" }
```

## Database diff-based save

For SQLite and DuckDB sources, `edit_table` preserves the **diff-based**
save semantics used everywhere in Octa: the reader snapshots each row's
identity on load, and only the rows that actually changed are written
back. In the example above, editing a cell in row 0 issues a single
`UPDATE`, the untouched row stays untouched, the deleted row issues a
`DELETE`, and the inserted row issues an `INSERT`. Column changes are
rejected before anything is written.

## Errors

| Situation                          | Message                                                |
|------------------------------------|--------------------------------------------------------|
| Row/column index out of range      | `set: row N is out of range (table has M row(s))`      |
| Unknown column name in `set`       | `set: no column named "..."`                           |
| Insert row arity mismatch          | `insert_rows: row has N cell(s) but the table has M column(s)` |
| Output format is read-only         | `format ... does not support writing - cannot edit ...` |

## See also

- [`write_table`](write_table.md) writes a new file from inline rows.
- [`read_table`](read_table.md) to inspect the file before/after editing.
