A release that makes database tabs tell the truth. A table opened from a
database holds only the pages it has loaded, and until now Summary, the
filters, Pivot, charts and every other analysis quietly worked on those pages
alone. They now run as SQL on the database, over the whole table, without
downloading it. New are **Hash columns** for building warehouse keys, **Load
whole table** for when you do want every row, and a timeline that zooms one
axis at a time. Unsaved edits now show in every view without a save, the
AppImage starts again on systems without the X11 keyboard library, and
nineteen languages got their missing accents back.

## What's new

### Hash columns

**Columns -> Hash columns...** adds one column holding a hash of the columns
you pick, row by row: the usual hash key of a data warehouse, such as
`MD5(UPPER(TRIM(first_name)) || '|' || birth_date)`.

- MD5, SHA-256 or SHA-512, as lowercase hex.
- The columns in the order you choose, joined with any delimiter (`|` by
  default), with your own text for empty cells, and optionally trimmed and
  upper-cased first.
- A preview shows the first rows' hashes; hover one to see the exact text
  that was hashed. **Edit -> Undo** removes the column again.
- On a database tab that holds only part of its table, the database computes
  the hash for every row, and every page you scroll to carries it.
- The same on the command line (`--hash-columns`) and over MCP
  (`hash_columns`).

### Load whole table

**Data -> Load whole table...**, also on the tab's right-click menu, counts
the rows of a partly loaded database table or file first, says how many there
are and how many the tab holds, and downloads the rest only when you click
**Load all**. The status bar shows the progress with **Cancel**, and
cancelling keeps every row that already arrived.

### Timeline: zoom one axis at a time

- **Time + / -** zooms only the time axis, **Lanes + / -** only the lanes,
  so a year of data fits on one screen with every lane still readable.
  **Fit** shows everything again.
- The label is written on each bar where it fits, not only shown on hover.
- The view stops at the first and last bar instead of scrolling on into empty
  years, and you cannot zoom out past the whole timeline.
- The time axis labels follow the zoom: years, then months, days and hours,
  always on calendar boundaries. Lane names stay on the left at any zoom.

## Changed

- **First and Last in Time series** now take the value at the earliest and
  latest time in each bucket, whatever order the rows are in. Before, they
  took the first and last row as the file happened to list them, so results
  can differ on unsorted data. The same on the command line and over MCP.
- **`db_relationships` over MCP**: `measure: true` now counts every row on
  the server instead of reading a sample, so its `sample` parameter is gone.
  A clean result is now proof, not just evidence.

## Fixes

### Analyses on database tabs used only the loaded rows

A database tab holds the pages it has loaded (100,000 rows each by default,
under **Settings -> Performance**), and every analysis ran on those pages as
if they were the whole table. With a table of millions of rows, Summary
described the first page and called it the table. Now, whenever
the tab does not hold every row, the database does the work over the whole
table, and nothing is downloaded for it:

- **Summary, Value frequency, the Data quality report and Correlation** are
  counted on the server.
- **The Join key finder, Join diagnostics, Find lookup tables** and the
  relationship map's **Measure** compare keys on the server. The Join key
  finder shows the row counts and what the check will cost before it runs.
- **Sorting, every row filter and the search box** run on the server, and
  the rows arrive already sorted and filtered, page by page. The column
  filter lists the most common values counted over the whole table, and
  finds values that are not loaded.
- **Random sample** picks from the whole table, either exactly (every row
  equally likely, which reads the whole table) or fast (the engine's own
  block sampling, about the size you asked for).
- **Pivot, Time series and Chart** are computed on the server. A line or
  scatter chart over many rows draws evenly spaced rows from the whole range,
  not just the first page.
- Each result says so in a line above it: "Computed on the database over all
  4,812,331 rows."
- What an engine's SQL cannot express runs on the loaded rows and is named
  in that line, with **Load whole table...** right there. MySQL, for example,
  has no percentile function, so its median and quartiles come from the loaded
  rows.
- Unsaved edits in the tab are not on the server yet, so a sort or filter
  that would fetch the rows again asks first: save, use only the loaded rows,
  or cancel.
- Long queries show a spinner with **Cancel**. A refused query shows the
  server's own message, with **Run on loaded rows**.
- **Settings -> Databases -> Run analyses on the database** switches all of
  this off. The results then say how many rows they cover.

The page [Analyses on live databases](https://thorstenfoltz.github.io/octa/usage/analyses-on-live-databases/)
lists, per engine, what runs where.

### More fixes

- **Unsaved edits now show in every view.** A cell changed in the Table view
  shows in Raw text, the JSON and YAML trees, Markdown and the Compare text
  diff as soon as you switch, and text edited in Raw text shows in the Table,
  Record, Chart, Timeline and Map views. Before, every view but the one you
  edited in showed the file as it was saved. Save writes whichever side you
  changed last. Text that no longer reads as the file's format keeps you in
  the text view, with the reason, until you fix or undo it.
- **The AppImage did not start** on systems without the
  `libxkbcommon-x11` package: Octa 0.20.1 stopped at once with "Library
  libxkbcommon-x11.so could not be loaded". The AppImage now carries the
  library itself.
- **Ask in the SQL editor** refused to work when the editor ran on a
  database connection other than the tab's own. It now describes that
  server's tables (those in the schemas you opened in the Databases sidebar)
  and their foreign keys to the model, so the query it writes uses real names.
- **Databricks** refuses a column or table name such as `my-col` written
  without quotes. Octa now quotes the name the error points at and runs the
  query again, in the SQL editor, over MCP (`query_db`) and on the command line
  (`--db-query`).
- **Relationship map**: long table names are shortened to fit their box
  instead of running over the edge (hover for the full name), and every list
  of tabs, schemas and tables has **All** and **None** buttons.
- **The filter popup on a column header** lost its Find box after the first
  letter typed.
- **Missing accents** in Danish, Norwegian, Swedish, Estonian, Lithuanian,
  Latvian, French, Slovenian, Croatian, Spanish, Portuguese, Finnish, German,
  Italian, Dutch, Turkish, Czech, Slovak and Hungarian: about 1,650 strings
  had been written without them.
- **The CSV toolbar in Raw text** lined its fields up unevenly.

## Under the hood

- Hash columns use the `md-5` crate (MIT or Apache-2.0) for MD5. It was
  already part of the build through two other dependencies, so no new crate
  is downloaded or shipped.
- Every analysis that runs on a database is tested against the same
  analysis on the same rows in memory, through a DuckDB-backed stand-in for
  a Postgres server, and against real PostgreSQL 17 and MySQL 8 servers in CI.
- The documentation covers everything above in both the in-app Help and the
  website, with new pages for analyses on live databases, Load whole table and
  Hash columns (GUI, command line and MCP).
