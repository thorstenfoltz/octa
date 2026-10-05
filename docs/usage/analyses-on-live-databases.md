# Analyses on live databases

A table opened from a database connection holds only the pages it has
loaded. When it does not hold every row, **Summary**, **Value frequency**,
the **Data quality report**, **Correlation**, the **Join key finder**,
**Join diagnostics**, **Find lookup tables** and the relationship map's
**Measure** run as SQL on the database instead, over the whole table, and
nothing is downloaded for it. Sorting, the row filters and **Random sample**
go there too. So do **Pivot**, **Time series**, a **Chart** opened from such
a tab and **Hash columns**.

The result tab says so in a line above the table: "Computed on the database
over all 4,812,331 rows."

Switch this off in **Settings -> Databases -> Run analyses on the
database**. Then these analyses run on the loaded rows, and the result says
how many that was.

## What runs where

Almost everything is counted on the server. Two kinds of part are not:

- **What the engine's SQL cannot express.** MySQL has no percentile
  function, so the median, quartiles, IQR and the quality report's outlier
  count come from the loaded rows there. Trino and Athena only have an
  approximate percentile, which is what they use.
- **What looks at individual values.** In the quality report, type
  consistency, PII, Benford's law, calendar gaps, value shapes and the extra
  report tabs come from the loaded rows on every engine.

The note above the result names each such part with its reason, and
offers **Load whole table...**: once every row is in the tab, Octa
computes everything itself, without SQL. The extra report tabs say above
their own table that they cover the loaded rows.

<!-- support-table:start -->
| Engine          | Median, quartiles, IQR, outliers | Correlation   | Join diagnostics: spaces and punctuation | Join diagnostics: leading zeros | Fast random sample |
|-----------------|----------------------------------|---------------|------------------------------------------|---------------------------------|--------------------|
| PostgreSQL      | server                           | server (CORR) | server                                   | server                          | block sampling     |
| MySQL / MariaDB | loaded rows                      | server (sums) | server                                   | server                          | exact only         |
| SQL Server      | server                           | server (sums) | loaded rows                              | loaded rows                     | block sampling     |
| Oracle          | server                           | server (CORR) | server                                   | server                          | block sampling     |
| Amazon Redshift | server                           | server (sums) | server                                   | server                          | exact only         |
| ClickHouse      | server                           | server (CORR) | server                                   | server                          | exact only         |
| Exasol          | server                           | server (CORR) | server                                   | server                          | exact only         |
| Trino           | server (approximate)             | server (CORR) | server                                   | server                          | block sampling     |
| Amazon Athena   | server (approximate)             | server (CORR) | server                                   | server                          | block sampling     |
| Snowflake       | server                           | server (CORR) | server                                   | server                          | block sampling     |
| Databricks      | server                           | server (CORR) | server                                   | server                          | exact only         |
| Google BigQuery | server                           | server (CORR) | server                                   | server                          | block sampling     |
<!-- support-table:end -->

## Keys and relationships

These compare tables, so they go to the database only when **every** table
involved is a database tab on the **same** connection (one statement cannot
join two servers). Otherwise they use the loaded rows, and the dialog says
why. Each dialog says "Computed on the database over every row." under a
result from the server.

- **Join key finder** first asks the database how big the tables are,
  from its statistics where it keeps them ("about 4,812,331 rows") or by
  counting. It then says how many column pairs there are, that an exact
  check reads every value of every column (a table with more than 64
  columns more than once), and that it can cost money on billed engines:
  BigQuery and Athena charge for the data a query reads, Snowflake for the
  time its warehouse runs. You choose: **Every pair, whole tables**
  (exact), **Only the likely pairs** (the pairs the loaded rows suggest,
  checked exactly on the database; usually cheaper, but each pair is its
  own query, so many pairs can cost more, and a key the loaded rows do not
  show is missed), **Loaded rows only**, or Cancel. The
  sample field is greyed out: the database reads every row, and the choices
  that use the loaded rows take every loaded row. The same holds for Join
  diagnostics.
- **Join diagnostics** counts matching keys, the samples of keys found on
  one side only, and every fix on the server: trimming spaces, ignoring
  case, collapsing repeated spaces, ignoring punctuation and ignoring
  leading zeros. SQL Server has no regular expressions, so collapsing
  spaces, ignoring punctuation and ignoring leading zeros are checked on
  the loaded rows there. They are listed under "Checked on the loaded rows
  only", and the note names them.
- **Find lookup tables** counts on the server. **Show breaking rows** and
  **Split out** then ask where the rows should come from: **Use the loaded
  rows** (fast, may miss some) or **Fetch from the database** (exact, one
  more query, which can take a while or cost money on billed engines).
  Fetched rows stop at your initial load limit, and the status bar says so
  when there were more. A fetched lookup takes each key's most common
  value; on a tie, the smaller value wins (Octa's own pass takes the one it
  saw first).
- **Relationship map, Measure** puts exact numbers on every line: for a
  declared (Database) map, and for a map of open tabs that all come from one
  connection. It re-measures the lines the map already shows; it does not
  look for new ones. The MCP tool `db_relationships` with `measure: true`
  gives the same numbers.

## Sorting, filtering and sampling

On a database tab that does not hold every row, sorting and every filter
that hides rows run on the database over the whole table: the sort (column
header, **Edit -> Sort**, **Sort by several columns...**, the sort an Ask
question sets), the value filters, the comparison filters and the search box
in Filter mode become the query that pages the tab. Scrolling then pages
through that result, and **Load whole table** loads it. The line above the
grid says what the database applied; **Clear sort** returns to the table's
own order. The analyses on this page then read the database's part of
that: what stays on the loaded rows (below) is not part of it, and under
**Only the loaded rows** they read the last result the database applied.

- **What stays on the loaded rows.** Regular-expression and whole-word
  search, a "greater than" or "less than" filter unless both the column and
  the value are numbers (in Octa each value falls back to comparing as text
  when it is not a number, and text order follows the database's
  collation), the duplicate filter and **Filter to marked**. The line above
  the grid names each one.
- **Typing waits for a pause.** The search box sends its text once you stop
  typing for a moment, not on every keystroke.
- **Unsaved edits.** A database sort or filter fetches a fresh result and
  replaces the loaded rows. With unsaved edits Octa asks first: **Save
  first**, **Only the loaded rows** (sort and filter what is loaded, as
  before), or Cancel. Save first is greyed out, with the reason, when the
  changes cannot be written back from the tab (a column removed or renamed,
  for example). Once the edits are saved, the tab sorts and filters on the
  database again.
- **When the database refuses.** The line above the grid shows its message
  with **Try again** and **Use the loaded rows**.
- **The value list.** The filter popup lists the 50 most common values over
  the whole table, counted on the database, and its search box searches
  every value there. The **Column Filter** window lists the 10,000 most
  common, and its Find field narrows that list. The Shapes view counts the
  loaded rows, so ticking only some shapes keeps the loaded values that have
  them.
- **Random sample** offers **Exact** (every row equally likely; the
  database reads and shuffles the whole table, which costs money on billed
  engines) and **Fast** (the engine's block sampling: quick and cheap, about
  the number of rows you asked for, and rows stored together tend to be
  picked together). Fast is offered where the table above says "block
  sampling", and not on a tab filtered on the database. On a view, on a
  table the database keeps no row count for, for a sample of half the table
  or more, or when the draw finds no rows, the exact sample runs instead,
  and the status line says so.
- **A database sort is a view, not an edit.** It is not on the undo list.
  Fetching the new rows clears the undo history, the selection, bookmarks
  and the colour marks on rows and cells, which pointed at the old rows.
  The tab is read-only until they arrive.

## Pivot and time series

On a database tab that does not hold every row, **Pivot** and **Time
series** run on the database over the whole table, and the result tab says
so. The preview in each dialog still comes from the loaded rows, and says
so: only **Run** (Pivot) and **Create tab** (Time series) go to the
database.

- **Pivot** first asks the database for the column's values, then counts
  or adds up each one. A pivot with more values than the engine allows
  columns in one query (about 1,000 on Oracle) is split into several
  queries and joined in Octa, each reading the table again. Column names
  are the database's text of each value; two values that differ only in
  case get `_1` on the second, as in Octa's own pivot.
- **Unpivot** stays on the loaded rows: it turns columns into rows, so the
  whole table would not fit in a tab. The result says so and offers **Load
  whole table**.
- **Time buckets** are made on the database when the time column is a date
  or time column there. A column the database stores as text is bucketed
  on the loaded rows, and the result says why. Weeks start on Monday on
  every engine.
- **First and Last** are the value at the earliest and latest time in each
  bucket, on a database tab and in a file alike.
- **Rolling window** asks first: **On the database** (every value exact;
  every row, or the first rows a tab holds, in time order) or **Use the
  loaded rows** (quick, but the loaded rows are the table's first pages,
  so a window may skip rows in between). On ClickHouse, First and Last
  windows run on the loaded rows only: ClickHouse skips empty values there.
- A result bigger than a tab holds says "Only the first N rows are here".

## Charts

A chart opened from a database tab that does not hold every row asks the
database for its picture, over the whole table, and says so above the
chart. It remembers the table and the tab's filter as they were when it
opened, so it keeps working after the tab is closed.

- **Histogram** counts every row into its bins. With automatic bins it may
  use more bins than the same table sampled in a file would, since it sees
  every row.
- **Bar** counts the categories first; more than **Chart max categories**
  (Settings) gives the usual message with the exact number. Bars are
  ordered by their X value: a database has no row order to keep.
- **Line** and **Scatter** draw evenly spaced rows across the whole table:
  the database sorts by X and returns every so many rows, up to **Chart
  max points**. The values are the rows' own, not averages. **Chart max
  points** set to 0 does not lift the limit here: the chart still draws at
  most 25,000 rows, since the table may be any size.
- **Box** asks for the quartiles and whiskers. MySQL has no percentile
  function, so there the box plot comes from the rows the chart copied when
  it opened, and says so.
- Changing the chart type, X, Y, aggregate or bins asks again after a short
  pause, with a spinner and Cancel in the status bar; the old chart stays
  until the new one arrives. Title, labels, legend, colours, axis ranges,
  log scale, trend and forecast never ask the database. Each new chart
  reads the whole table, which costs money on billed engines (BigQuery,
  Athena, Snowflake).
- If the database refuses, the chart shows why, with **Try again** and
  **Use the loaded rows**.
- Switching **Run analyses on the database** off makes an open chart draw
  from the rows it copied when it opened, from then on.

## Hash columns

**Columns > Hash columns...** on a database tab that does not hold every row
asks the database to compute the hash column, for every row. Adding it reads
the table again with the column filled, every later page and **Load whole
table** carry it, and the analyses above see it over the whole table. The
dialog's preview comes from the database too. The hash cells cannot be typed
into, a hash column is never written back, and **Refresh** reads the table
without it. Each value is the database's own text of the cell, so a hash can
differ from the same row in a file. Details are on the
[Hash columns](hash-columns.md#on-a-live-database) page.

## Differences you may notice

- **Values are the database's text.** Value frequency labels and the mode
  are the database's own text form of each value, so dates and decimals can
  look different from the grid.
- **Distinct counts compare text byte for byte**, as Octa does, so `a` and
  `A` are two values even on MySQL and on SQL Server's case-insensitive
  collations. SQL Server ignores trailing spaces, so there `a` and `a` with a
  trailing space count as one.
- **Sorting text ignores case and compares byte by byte**, as Octa does. On
  Oracle, a session sort setting other than `BINARY` can still order
  accented letters differently.
- **A value filter on SQL Server ignores trailing spaces**, so `abc` also
  keeps `abc` with a trailing space.
- **Spaces.** On SQL Server (and MySQL's older collations) a value of only
  spaces can count as empty (missing), and text length on SQL Server
  ignores trailing spaces.
- **Value frequency bins.** On a database tab the Bins field counts when you
  press Enter or click away, not on every keystroke. The dialog's footer says
  the values were counted on the database over all N rows. After a Cancel or a
  server error the dialog offers **Run on loaded rows**.
- **Correlation picks its columns from the loaded rows.** It chooses numeric
  columns the same way Octa's own correlation does, so NUMERIC and DECIMAL
  columns take part, and so does a text column whose loaded values read as
  numbers. If not every value of such a column converts to a number, the
  database reports an error. A true/false column takes part as 1 and 0, as
  it does in Octa's own correlation.
- **The quality score is a mix.** It combines server counts with type
  consistency from the loaded rows. Where outliers come from the loaded rows
  (MySQL), so does the outlier penalty.
- **Key matching is exact, except trailing spaces on SQL Server.** The key
  analyses compare text byte for byte on every engine, as Octa does, so
  `abc` and `ABC` are two keys even on MySQL and SQL Server. SQL Server
  ignores trailing spaces in every comparison, so there `abc` and `abc` with a
  trailing space are one key, and a key of only spaces joins the empty one.
- **Trimming means spaces.** The database's `TRIM` strips spaces only; Octa
  also strips tabs and line breaks.
- **Collapsing spaces and punctuation follow the database.** BigQuery,
  Databricks, Trino and Athena treat only ASCII characters as whitespace, so
  a non-breaking space is not collapsed there. Octa's own pass counts ASCII
  punctuation; on some engines the database's `[[:punct:]]` also covers
  other marks. On Oracle and Exasol, a value that is nothing but punctuation
  becomes empty, and those engines treat empty as missing, so it cannot
  match.
- **Athena needs engine version 3** to ignore leading zeros on the server.
- **Some column types cannot be grouped.** Find lookup tables groups by
  each column, which some engines refuse for JSON, XML, CLOB or ARRAY
  columns. The database's error shows, and **Run on loaded rows** still
  works.
- **Samples are sorted by the database.** The "only on the left / right"
  examples are the first five in the database's sort order.
- **MySQL needs version 8.0.17 or later** for these analyses, because they
  use `CAST ... AS DOUBLE`.
- **Outlier counts can differ by a value or two near the edge.** The server
  interpolates quartiles; Octa's own pass picks the nearest value.
- **Unsaved edits are not included.** The database has not seen them. The
  note says so when the source tab has any.

## Cancelling and errors

The analysis shows in the status bar with a Cancel button, which stops the
statement on the server. If the database refuses (a permission, a timeout),
Octa shows its message and offers **Run on loaded rows**. It never falls
back on its own.
