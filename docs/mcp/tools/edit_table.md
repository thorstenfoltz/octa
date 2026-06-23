# `edit_table`

Edit an existing tabular file in place: add computed columns, set individual
cells, insert rows, delete rows, and drop columns, then save back through the
file's native writer.

## When to use

- "Fix the value in row 3 of the `status` column."
- "Add these two rows to the table."
- "Delete rows 10 and 11."
- "Add a `running_total` column = `SUM(amount) OVER (ORDER BY id)`."
- "Drop the `notes` and `internal_id` columns."

For writing a brand-new file from inline data, use
[`write_table`](write_table.md). `edit_table` can **add** columns (see
`add_column`) and **drop** columns (see `drop_column`); to rename or retype
columns use [`transform_columns`](transform_columns.md).

!!! note "Write protection"
    Adding or removing a column on a DuckDB, SQLite, or GeoPackage file is a
    **schema change**. It is refused unless Write protection is off (the MCP
    server reads `write_protection` once at startup; restart after changing
    it). Adding a column to a plain file format (CSV, Parquet, ...) is always
    allowed.

## Input schema

| Parameter     | Type   | Required? | Default      | Description                                                                    |
|---------------|--------|-----------|--------------|--------------------------------------------------------------------------------|
| `path`        | string | yes       | (no default) | File to edit, in place                                                         |
| `table`       | string | no        | (no default) | For multi-table sources (SQLite, DuckDB, GeoPackage, Excel), which table       |
| `set`         | array  | no        | `[]`         | Cell edits: `{ "row": int, "col": int \| string, "value": any }`               |
| `insert_rows` | array  | no        | `[]`         | Rows to insert: `{ "at"?: int, "values": [...] }`                              |
| `delete_rows` | array  | no        | `[]`         | 0-based row indices to delete                                                  |
| `add_column`  | array  | no        | `[]`         | Columns to append: `{ "name": string, "expression": string }`                 |
| `drop_column` | array  | no        | `[]`         | Columns to remove, each a 0-based index or a column name                       |
| `unlimited`   | bool   | no        | `false`      | Load the whole file before editing (and rewrite it in full for non-DB formats) |

- `set[].col` is either a 0-based column **index** or a column **name**.
- `set[].row` and `delete_rows[]` are 0-based indices into the loaded rows.
- `insert_rows[].at` is the 0-based insertion index; omit it to append at
  the end. `values` line up positionally with the columns.
- Cell values are coerced to the target column's type, the same way
  [`write_table`](write_table.md) coerces them.
- `add_column[].expression` is a DuckDB SQL expression evaluated per row
  against the loaded table, either scalar (`v * 2`) or a window function
  (`AVG(v) OVER (ORDER BY id ROWS BETWEEN 6 PRECEDING AND CURRENT ROW)`).

- `drop_column[]` entries are column indices or names. Columns are dropped
  **last** (see the order below), so every other op still refers to the
  file's original columns. You cannot drop every column.

Operations are applied in a fixed order regardless of how you list them:
columns are added first, then rows are inserted, then cells are set, then
rows are deleted (highest index first, so lower indices stay valid), then
columns are dropped (highest index first).

## Response shape

```json
{
  "columns_added": <n>,
  "cells_set": <n>,
  "rows_inserted": <n>,
  "rows_deleted": <n>,
  "columns_dropped": <n>,
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
{ "columns_added": 0, "cells_set": 1, "rows_inserted": 1, "rows_deleted": 1, "columns_dropped": 0, "path": "/data/app.sqlite" }
```

### Drop two columns

```json
{
  "name": "edit_table",
  "arguments": {
    "path": "/data/people.parquet",
    "drop_column": ["notes", 0]
  }
}
```

## Database diff-based save

For SQLite and DuckDB sources, `edit_table` preserves the **diff-based**
save semantics used everywhere in Octa: the reader snapshots each row's
identity on load, and only the rows that actually changed are written
back. In the example above, editing a cell in row 0 issues a single
`UPDATE`, the untouched row stays untouched, the deleted row issues a
`DELETE`, and the inserted row issues an `INSERT`.

Adding a column with `add_column` or removing one with `drop_column` is a
schema change. When Write protection is off, Octa reconciles the database
table to the new column set and the diff-save then refills the values,
preserving every row's identity (indexes, constraints, and triggers are not
preserved). When Write protection is on, the column change is refused before
anything is written.

## Errors

| Situation                     | Message                                                        |
|-------------------------------|----------------------------------------------------------------|
| Row/column index out of range | `set: row N is out of range (table has M row(s))`              |
| Unknown column name in `set`  | `set: no column named "..."`                                   |
| Insert row arity mismatch     | `insert_rows: row has N cell(s) but the table has M column(s)` |
| Output format is read-only    | `format ... does not support writing - cannot edit ...`        |

## See also

- [`write_table`](write_table.md) writes a new file from inline rows.
- [`read_table`](read_table.md) to inspect the file before/after editing.
