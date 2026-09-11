A maintenance release. Exported XLSX dates were text, zoned timestamps showed
the wrong hour, Databricks could be attached but never queried, the assistant
described a billion-row file from its first 2,000 rows, and thirty dialogs
could not be moved. All of that is fixed. On the way, a blank table can now be
started from the File menu or from a `CREATE TABLE`, duplicates have one dialog
instead of three menu entries, and rows fit their content the way columns
already did.

## What's new

### Start a table from nothing

**File > New Table...** asks how many columns and rows to begin with (3 x 1 by
default) and opens a blank, editable grid in a new tab, the way a spreadsheet
opens an empty sheet. Columns are named `col1`, `col2`, ...; rename them, add
rows or columns, then save to any writable format. Nothing exists on disk until
you save. **File > New File...** keeps doing what it did, an empty text tab to
type or paste into, and its hover text now says so instead of promising a
table.

The SQL panel can do the same from a statement. `CREATE TABLE people (id
INTEGER, name VARCHAR)` opens an empty tab named `people` with those columns
and types; `CREATE TABLE top AS SELECT * FROM data ORDER BY score DESC LIMIT
10` opens a tab holding those rows. It works from an empty tab with nothing
open, and read-only mode does not block it, because a new tab is not an edit.
The table is handed over rather than kept, so it is not queryable from the
original panel afterwards; switch to the new tab and it is `data` there.

### One dialog for duplicates

**Data > Find duplicates...** now does everything the three former entries did.
Pick the key columns, then choose what happens: highlight the repeats, open
them in a new tab, **show only the duplicate rows**, **show only the rows that
occur once**, or **drop them**, keeping the first or the last occurrence, as one
undo step. The two filter modes are new: they narrow the table itself and leave
a removable chip above it, and they remember the key columns rather than row
numbers, so the filter stays right after you edit, insert or delete rows.

**Find near-duplicates...** sits beside it in the Data menu. The separate
**Drop duplicate rows...** entry is gone; its shortcut, **Ctrl+Shift+H**, opens
the same dialog preset to drop with every column ticked.

### Rows fit their content, like columns

Drag a row's bottom edge in the row-number gutter to change its height, or
drag the bottom edge of the `#` corner to set every row at once. Double-click
either seam to fit the row, or all rows, to their content, and **Edit >
Auto-fit All Rows** does the same from the menu, the twin of Auto-fit All
Columns. Fitting a row means showing its whole value, so it switches
**Settings > Table > Cell line breaks** on if it was off. Heights are per tab
and session-only, and the "set every row" corner stores one number, so a table
with millions of rows costs nothing.

### The assistant reads better and can stay out of your data

Replies render as Markdown: headings, lists, tables, code blocks and links,
instead of the raw source. **Ctrl+C** on a reply still copies plain text.

A **Just answer** toggle in the panel header sends a message with no tools and
no data at all: the assistant never sees what you have open that turn, and the
request costs a fraction of the tokens. **Data** is the normal mode and stays
the default. A write-enabled profile can now also **sort** the open tab by one
or more columns, as one undo step.

### Cloud objects in the SQL workspace

**Attach cloud** in the SQL panel lists your saved cloud connections, opens a
picker for that bucket or container, and registers the objects you tick as
workspace tables. Combining them into one table is a separate, opt-in choice.

### XLSX saves carry what you see

Every `.xlsx` save now writes real dates and timestamps (they were text),
column widths, a bold header row, an autofilter on the header, clickable
hyperlinks for URL cells, and your data-validation rules as Excel validation.
Two new settings, both off by default, under **Settings > File-Specific**:
**Document properties** stamps the workbook with a title and author, and
**Write as an Excel table** writes a real table object instead of a plain
range. Sheets past 500,000 rows are written in constant-memory mode, so a big
export no longer needs the whole workbook in RAM.

## Fixes

- **Exported dates were text in XLSX.** A date or timestamp column fell through
  to the string writer, so a spreadsheet could not sort or compute on it. They
  are real Excel dates now, with a date format on the column.
- **Zoned timestamps showed the wrong hour.** A Parquet, ORC or Arrow column
  whose header said `Europe/Brussels` rendered in UTC, two hours behind its own
  header; DuckDB `TIMESTAMPTZ` columns did the same. Both paths now convert into
  the named zone, and a save converts back out of it, so a round trip moves
  neither the clock nor the zone. A first query on a cold DuckDB connection
  used to report UTC regardless of the session zone; the connection is warmed
  so it does not.
- **Naive timestamps were exported as zoned columns.** Schema export in seven
  SQL dialects, and the live-database write-back that shares the code, decided
  "zoned" from one spelling of the type name, so Parquet's `Timestamp(us)` and
  ORC's plain `Timestamp` became `TIMESTAMPTZ` / `DATETIMEOFFSET` /
  `TIMESTAMP_TZ`. One shared predicate decides now.
- **Databricks attached but showed no table.** The attach enumerated schemas
  without the catalog level that Databricks, Snowflake, BigQuery and Trino
  need, so the menu listed everything and the import brought nothing. The
  attach menu now drills `catalog > schema > table` for every import engine,
  so the usual answer is one table and nothing large is enumerated.
- **"Unlimited" rows broke every server-side LIMIT.** The Unlimited row cap
  (Settings > Performance, `--rows all`, MCP `unlimited`) was printed into
  `LIMIT 18446744073709551615`, which Databricks types as `DECIMAL(20,0)` and
  refuses, and Postgres would overflow. An unlimited cap now emits no clause at
  all; the same sentinel was swept out of ClickHouse, Exasol and BigQuery.
- **An imported table showed up twice, under an unusable name.** Importing a
  table into the SQL workspace also listed a phantom attachment labelled
  `[MSSQL]` whatever the engine was, and named the table
  `warehouse_fabric_sales_orders__orders`. An import records no attachment
  now, the table is named after itself (`orders`, `orders_2` on a clash) with
  its provenance on hover, and a double-click on the name renames it.
- **Ask SQL was greyed on the tab where it mattered.** An empty tab with a
  database attached in its SQL panel is exactly where a question about that
  database is asked, and Ask refused it for having "no columns". It works
  there now, the prompt names the attached and registered tables so the model
  can query them, and a profile picker beside the box chooses which assistant
  answers.
- **The token meter ignored the Ask boxes.** Ask filter and Ask SQL each make
  their own request and counted nothing; they count now, and the meter's hint
  says plainly what the number includes.
- **Explain file described a billion-row file from its first 2,000 rows.** In
  large-file mode the tab holds one page of the file, and the assistant's
  tools took that page for the table. Tools now read the whole file from disk
  and the tab summary says which rows the window shows.
- **Thirty dialogs could not be moved.** Every centred window ignored a drag
  (an anchored egui window is immovable), and Update, AI report and Repair
  could not be resized either. All of them now open centred, then move, resize,
  minimise and maximise like the rest.
- **The SQL toolbar was staggered.** The Ask box sat below the buttons beside
  it, and so did everything after it. The box, its button and the profile
  picker have a row of their own, on one centre line, and the box grows
  downwards as a question wraps.
- **A dead "Write options" row** in Settings > File-Specific had a label and
  no control. Removed; the real group is under Settings > Files.
- **New File's hover text** promised "a new empty table" and delivered a text
  editor. It now describes the text editor; New Table is the entry for the
  grid.

## Breaking changes

- **Drop duplicate rows... is no longer a menu entry.** It is the drop mode of
  Data > Find duplicates...; Ctrl+Shift+H still opens it. Find duplicates and
  Find near-duplicates moved from the Search menu to the Data menu.
- **`CREATE TABLE` in `--sql` and the `run_sql` MCP tool** returns the created
  table's rows (with its declared types) instead of the contents of `data`,
  reports `created <name>` on stderr (`"created": "<name>"` in the MCP
  response), and drops the table again afterwards. A script that created a
  table and queried it in a later call has to do both in one statement now.
- **Imported workspace tables are named after themselves.** A saved snippet
  that referenced `alias__table` needs the new, shorter name.
- **The `ask_enable_line_breaks` setting is gone**, together with the "Wrap the
  text too?" prompt it governed. An existing `settings.toml` that still carries
  the key loads fine; the key is ignored.

## Under the hood

- **A created table is found by diffing DuckDB's catalog** before and after the
  statement, not by parsing the SQL, so CTAS, views, quoted and `TEMP` names
  all count and a `CREATE` inside an attached database (read-only anyway) never
  does.
- **Menu popups froze at the size they opened with.** An egui popup is an
  auto-sized area, and a scroll area inside it never asks to grow, so a
  submenu opened while its listing said "Loading..." stayed two entries tall
  however many the server sent. The attach menus size their content from the
  entries; a headless test drives a real nested menu and fails on the old
  layout.
- **Alignment is measured, not reasoned.** Three separate causes put the Ask
  row's box, button and combo on three centre lines, and none of them shows in
  a toy reproduction with egui's default style. A guard test drives the row
  headlessly and names the offending widget.
- **XLSX styling has a `formatting` flag** instead of encoding "off" as "no
  style", because widths and validation now travel on every save whether
  formatting is on or not. Autofit stays off for a streamed sheet: in
  constant-memory mode it would size every column from the last row alone.
