# Header row

Some files do not start with their column names. Bank and government exports
often put a title block above the real header, Excel reports spread the
header over two rows with merged cells, and some files have no header at
all. Octa opens them as they are and lets you fix the header afterwards.

## Use a row as the header

Right-click any cell in the row that holds the column names and pick **Use as
header row**. That row becomes the column names, and it and every row above
it are removed.

When the header spans two rows (a merged `2025` above `Q1 Q2 Q3 Q4`), select
both rows first: click the first row number, Shift+click the second, then
right-click. The entry reads **Use rows 4-5 as header** and combines them. A
blank cell in the upper row takes the text to its left, so the merged `2025`
reaches every quarter below it and the names become `2025 Q1`, `2025 Q2`, and
so on.

Blank names become `column_1`, `column_2`, ...; a name that repeats gets
`_2`, `_3`.

Afterwards Octa looks at every column again, the way it does when a file
opens: a column that was text only because of the title rows becomes numbers
or dates again, European numbers like `1.234,56` included. A column that
could be read two ways, such as dates that work day first and month first,
stays text; **Change column type** lets you pick.

**Ctrl+Z** puts everything back in one step: the rows, the names and the
types.

## Octa suggests the row

When most column names are placeholders such as `column_3` or `Column3`, the
file almost certainly has a title block. Octa then looks through the first
20 rows for one that is filled across and made of words, and shows a banner:

> The column names look like they are in row 4. **Use row 4 as header**
> **Dismiss**

Nothing changes until you click. For a merged two-row header the banner
offers both rows.

## A file without a header

If the first row of the file is data, Octa has used it as the column names.
Untick **Edit > First row is header**: the names go back into the table as
the first row, and the columns are called `column_1`, `column_2`, and so on.
Tick it again, or press Ctrl+Z, to undo that.

## When it is not available

- In read-only mode and in large-file mode, like every other edit.
- On database tables: their column names come from the database.
- While a filter hides rows: rows above the header that you cannot see would
  be removed. Clear the filter first. A sort is fine, because the rows above
  are then the rows you see above.

## Next month's file

Both actions are recorded as [Recipe](recipes.md) steps (`use_header_rows`,
`header_is_data`), so replaying the recipe on next month's export fixes its
header the same way: in Octa, with `octa --recipe`, and over MCP. The step
remembers the row number, so it fits files whose title block has the same
height.

## Title lines in CSV files

A CSV whose first line is a title (`Report generated 03.10.2026`) used to open
one column wide, because every row was sized to that line. Octa now sizes the
table to the rows below it and finds the separator from the lines that agree
with each other, so a `;` file with a title line opens with every field in
place. Columns that are empty in every row, because each line ends in a
separator, are left out as before.
