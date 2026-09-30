A release for the SQL editor, and for computers less powerful than the one
Octa is built on. The SQL panel can now run a query on any saved database
connection from any tab, runs only the part you marked, comments lines out with
F12, formats queries in the style you pick, shares one query history across
every tab, and copies any selection out of its result grid. Several editors can
sit side by side, the editor stays open when its tab closes, and a running query
is plain to see. The table can copy a selection as a SQL IN list and show
invisible characters, and Octa can reopen your last session. Copy and paste now
work on Wayland. The default limits are lower, so a big file no longer fills the
memory of an ordinary laptop.

## What's new

### Run on any connection

The SQL panel has a **Run on** picker on every tab: **local DuckDB**, or any
saved database connection. Picking a connection sends the query straight to
that server in its own SQL dialect, where every table can be queried by its
real name. Nothing is attached or copied, so an empty SQL editor is now the
quickest way to query a database. A tab opened from a database starts on its
own connection. **Attach connection** is still there for the one thing this
cannot do: joining a server's tables with local files.

### Run the marked part

Mark part of the editor and Ctrl+Enter runs only that part, so several
statements can live in one editor and run one at a time. A **Run** button in
the toolbar does the same.

### Comments

- Everything after `--` on a line is drawn faded, so what runs stands out from
  what does not. A `--` inside a quoted string is text, not a comment.
- **F12** comments the marked lines out, or back in when they all already
  start with `--`. Press it twice and you are back where you started. Rebind
  it under **Settings -> Shortcuts**.

### One query history

Every SQL editor now shows the same history, whatever the tab or connection,
and it is kept between sessions. Each entry says where it ran (the
connection, the cloud object or the file), how long it took and how many rows
it returned. Picking an entry loads the text into the editor and leaves where
it runs alone. The per-connection lists from earlier versions are merged into
the one list on first start.

### Copying from the result

Click a cell to select it, Ctrl+click to add more, Shift+click for a
rectangle, a column header for the whole column and a row number for the
whole row. **Ctrl+C** copies the selection as tab-separated text, ready to
paste into a spreadsheet. Right-click a cell for Copy cell, row, column,
selection or all.

### Better autocomplete

- Catalog and schema names are offered, and each part of a dotted name
  (`wh.sales.orders`) completes on its own.
- On a server connection, the server's catalogs, schemas and tables are
  offered, and the columns of every table the query names are fetched in the
  background, so `SELECT o.` lists the columns of `orders`.
- Names with accents or umlauts complete like any other, and the list scrolls
  when more names match than fit.
- The faded line in an empty editor is a template: press **Tab** and it
  becomes real text, ready for Ctrl+Enter.

### Format SQL, in your style

**Format** in the SQL toolbar lays a query out one clause per line, indented,
so a long one-liner becomes readable. With part of the editor marked, only that
part is formatted. **Settings -> SQL -> Format SQL** decides the style, with a
live preview:

- keywords in upper case, lower case or as written
- 2 spaces, 4 spaces or a tab
- commas at the end of a line or at the start of the next
- JOIN level with FROM or under it
- how long a list, a clause (`GROUP BY c.name`) and a bracket
  (`coalesce(a, 0)`) may be and still stay on one line, each explained with an
  example right under its field
- blank lines between statements
- a closing semicolon
- the whole query on one line, which only tidies spacing and keyword case

### SQL settings in groups

**Settings -> SQL** is no longer one long list. Its settings sit in six groups
you open when you need them: Panel, Editor, Results, Other open tabs, Query
history and Format SQL.

### Open result as tab

**Open result as tab...** now puts every row of the result into the new tab,
not only the page on screen, and always opens a new tab, so the SQL panel's
own tab is never replaced.

### Several SQL editors side by side

**+** in the SQL toolbar, or **Ctrl+T** (also while typing), opens another
editor next to the others, each with its own query and its own result. Click
into one to make it the one Run, Format and Export act on; the small x above an
editor closes it.

### The SQL editor stays open

Closing a tab or opening a file no longer closes the SQL editor: the next tab
opens it and takes over your queries when its own editor is empty, so closing
the last tab does not lose an unsaved query. **Settings -> SQL -> Keep the SQL
editor open** switches back to the old behaviour.

Opened on an empty window, the editor fills it. **Maximise** and **Restore** in
its header switch between the whole window and docked beside the table.

### You can see a query running

A running query clears the previous result and error at once and shows a large
spinner with the time so far, with **Cancel** for a query on a database server.
Before, the old result stayed on screen until the new one arrived, which looked
like the answer to the new query.

### Sidebars where you want them

The Cloud and Databases browsers each have their own dock position under
**Settings -> Panels**, left by default. Browsers on the same edge share one
panel.

### Copy as IN list

Mark some cells, right-click, **Copy as IN list**, and you get
`('A-17', 'B-22', 'C-09')`, ready to paste after `WHERE id IN` in any database
tool. Empty cells and repeats are left out, quotes are escaped, and numbers stay
bare only when every value is a number, so `007` keeps its leading zero.

### Show invisible characters

**View -> Show invisible characters** marks the whitespace inside cells: `.`
for a space, `->` for a tab, `_` for a non-breaking or other unusual space, `|`
for a zero-width character. Spaces at the start or end of a value and the
unusual characters get a coloured background, which explains why `Berlin` and
`Berlin` did not match, and you can see at a glance whether a gap is spaces or
a tab.

### Reopen last session

**Settings -> Files -> Reopen last session** opens the files again that were
open when you closed Octa, the way a browser restores its tabs. Cloud objects
and database or API tabs stay closed unless you switch them on separately,
since reopening them downloads or queries at every start.

## Changed defaults

Octa was tuned on a fast machine. These defaults suit an ordinary laptop
better. Only new installs get them: if you already saved your settings, your
values stay as they are, and every one of them can be changed under
**Settings -> Performance**.

- **Rows loaded when a file opens**: 2,000,000 instead of 5,000,000. The same
  limit applies to SQL results, database tables, the command line and the
  assistant. `--rows all` and Unlimited still load everything.
- **Large-file mode** starts at 2 GB instead of 10 GB, so a big CSV, JSON or
  Parquet file is read from disk instead of being loaded into memory.
- **Raw view** reads files up to 50 MB instead of 500 MB.
- **Charts** plot up to 25,000 points before sampling, instead of 100,000.
  The shape of the data stays the same, and the chart redraws much faster.

## Fixes

- **Copy and paste on Wayland**: copying from Octa and pasting into another
  program (or the other way round) did not work, because Octa used the X11
  clipboard. Octa now uses the same clipboard as its text fields, on Wayland
  and on X11 alike. Paste also no longer lands in the table while you are
  typing in a text field.
- **The SQL result grid twitched** while scrolling, as its columns kept
  resizing to fit the rows on screen. Column widths now stay put.

## Under the hood

- SQL Server connections use tiberius 0.13 from crates.io instead of Octa's
  own patched copy. Long queries on SQL Server are not cut off: 0.13 would
  stop any query after 30 seconds of silence from the server, and Octa turns
  that off.
- The documentation covers the new SQL features and the new defaults in both
  the in-app Help and the website.
