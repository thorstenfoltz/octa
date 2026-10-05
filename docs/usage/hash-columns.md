# Hash Columns

**Columns > Hash columns...** adds one column holding a hash of the columns
you pick, row by row. It is the usual way to build a hash key in a data
warehouse, for example `MD5(UPPER(TRIM(first_name)) || '|' || birth_date)`.

Any column type works. Each value is turned into text first, the way Octa
shows it in the grid, then the values are joined and hashed.

## Options

| Option                | What it does                                                                                                                                                  |
|-----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Columns, in order** | Pick columns with **Add a column**; move them with `^` / `v`, take one out with `x`. The order is part of the hash: `a, b` and `b, a` give different results. |
| **Algorithm**         | MD5 (32 hex characters, the default), SHA-256 (64) or SHA-512 (128).                                                                                          |
| **Delimiter**         | Put between the values, `\|` by default. Any text, also empty. Without one, `ab` + `c` and `a` + `bc` hash the same, which is why a delimiter is the default. |
| **NULL as**           | The text that stands in for an empty cell. Empty by default; some teams use a marker such as `<NULL>` so that an empty cell and an empty string differ.       |
| **Trim whitespace**   | Strip spaces at the start and end of each value first.                                                                                                        |
| **Upper-case**        | Upper-case each value first, so `abc` and `ABC` hash the same.                                                                                                |
| **New column**        | The name of the result, `hash_<columns>` by default. It must not exist yet.                                                                                   |

The **Preview** shows the first rows' hashes. Hover one to see the exact text
that was hashed. The hash is lowercase hex. **Edit > Undo** removes the
column again.

## On a live database

On a database tab that holds only part of its table, the database computes
the hash, for every row:

- **Add column** reads the table again from the database with the hash
  filled in, so the selection, marks and **Undo** history start fresh. Every
  page you scroll to, and **Load whole table**, carries the hash too. Save or
  undo your changes in the tab first; until then **Add column** is greyed.
- The **Preview** comes from the database: a short pause after you change a
  setting, it shows the hashes of the first rows the database returns.
- Analyses that run on the database (Summary, Value frequency, the filters,
  sorting) see the hash column over every row, so finding duplicate keys
  covers the whole table.
- The hash cells cannot be typed into. Other changes to them, such as Find
  and replace, are never saved, and the next read from the database puts the
  database's hash back. Delete the column to remove it; **Refresh** reads the
  table fresh, without it.
- Each value is the database's own text of the cell. Dates and decimals can
  look different from the grid, so a hash can differ from the same row in a
  file. **Trim whitespace** strips spaces only on the database. SQL Server
  needs version 2019 or later (UTF-8 text).
- If the database refuses, the table shows why, with **Try again** and **Use
  the loaded rows**; with the loaded rows the hash column stays empty. So it
  does after switching **Run analyses on the database** off, which reads the
  plain table again.

## Good to know

- A tab that holds only part of its source (a file cut at the row limit, or a
  database table with **Run analyses on the database** switched off) cannot
  be hashed yet: the result would cover the loaded rows only. Use **Data >
  Load whole table...** first.
- A database renders dates and decimals as text its own way, so the same
  values can hash differently in Octa and in a database query. When the two
  must match, cast to text yourself in SQL in the same format Octa shows.
- The same function is available from the command line as
  [`--hash-columns`](../cli/hash-columns.md) and to AI agents as the MCP tool
  [`hash_columns`](../mcp/tools/hash_columns.md).
