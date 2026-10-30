# Edit and reshape

Changing cells, columns and the shape of a table, with undo for all of it.

![Edit and reshape](../../assets/screenshots/insert-column-formula.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                                 | In the app                                                                        | Command line                      | MCP tool                                                    |
|----------------------------------------------|-----------------------------------------------------------------------------------|-----------------------------------|-------------------------------------------------------------|
| Edit cells, rows and columns                 | [Editing](../../usage/editing.md)                                                 | -                                 | [`edit_table`](../../mcp/tools/edit_table.md)               |
| Lock a table against edits                   | [Read-only mode](../../usage/editing.md#read-only-mode)                           | -                                 | -                                                           |
| Find and replace text                        | [Find & Replace](../../usage/search-and-filter.md#find-replace)                   | -                                 | -                                                           |
| See what you changed                         | [Edit Audit Trail](../../usage/edit-audit-trail.md)                               | -                                 | -                                                           |
| Add a column from a formula                  | [Formulas](../../usage/formulas.md)                                               | -                                 | -                                                           |
| Calculate with dates and times               | [Date / Time calculation](../../usage/date-time-calculation.md)                   | -                                 | -                                                           |
| Work with clock times and durations          | [Times and durations](../../usage/times-and-durations.md)                         | -                                 | [`transform_columns`](../../mcp/tools/transform_columns.md) |
| Transform a column (split, trim, replace...) | [Transform Column](../../usage/transform-column.md)                               | -                                 | -                                                           |
| Rename columns                               | [Rename columns](../../usage/rename-columns.md)                                   | -                                 | [`transform_columns`](../../mcp/tools/transform_columns.md) |
| Change a column's type                       | [Change column type](../../usage/change-column-type.md)                           | -                                 | [`transform_columns`](../../mcp/tools/transform_columns.md) |
| Turn every date column into dates            | [Change column type](../../usage/change-column-type.md#every-date-column-at-once) | -                                 | -                                                           |
| Pivot and unpivot                            | [Pivot / Unpivot](../../usage/pivot.md)                                           | -                                 | [`pivot`](../../mcp/tools/pivot.md)                         |
| Transpose, sample, compare rows              | [Table Tools](../../usage/table-tools.md)                                         | [`--sample`](../../cli/sample.md) | [`sample`](../../mcp/tools/sample.md)                       |
