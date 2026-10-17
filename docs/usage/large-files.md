# Large Files

Large-file mode opens a file **read-only**, keeping its rows on disk and
fetching only what is on screen. That is the trade: you reach every row
of a file far bigger than memory, and in exchange the tab cannot be
edited.

Everything that changes data is off in this mode, and every feature that
works on "the table" works on the one page of rows on screen. The table
below lists what changes, feature by feature.

## Ordinary open or large-file mode

An ordinary open reads the first rows into memory (up to **Settings ->
Performance -> Initial-load row cap**, 2,000,000 by default) and streams
more in as you scroll down. Large-file mode keeps the rows on disk and
holds one page of 2,000 rows at a time.

| Feature                                                                          | Ordinary open                                                               | Large-file mode                                                                |
|----------------------------------------------------------------------------------|-----------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| Rows in memory                                                                   | Up to the initial-load row cap, more as you scroll                          | One page of 2,000 rows                                                         |
| Scrolling                                                                        | Through the loaded rows; more load at the bottom                            | Anywhere in the file; the scrollbar spans all of it                            |
| Editing cells, inserting or deleting rows and columns, undo                      | Yes                                                                         | No, the tab is read-only                                                       |
| Colour marks, conditional formatting, validation highlighting, find and replace  | Yes                                                                         | No                                                                             |
| Change type, Detect dates, Use as header, Transform and the other column tools   | Yes                                                                         | No, they change data                                                           |
| Search box                                                                       | Filters or highlights the loaded rows                                       | Filters the whole file, once typing pauses; highlighting does not apply        |
| Column Filter, Filter by value                                                   | Yes                                                                         | Filter the whole file; their value lists count the whole file (see below)      |
| Filter to marked                                                                 | Yes                                                                         | No marks in this mode                                                          |
| [Ask](search-and-filter.md) (plain-language filter)                              | Yes                                                                         | Filters the whole file                                                         |
| Sort by a column header                                                          | The loaded rows                                                             | The whole file, letter by letter (Natural text order is not applied)           |
| Sort by columns... (several keys)                                                | Yes                                                                         | Reorders only the rows on screen                                               |
| [SQL panel](sql.md)                                                              | `data` is the loaded rows                                                   | `data` is the whole file, read in place (see below)                            |
| [Summary](summary.md), [Chart](chart.md), Value frequency and the other analyses | The loaded rows                                                             | The page on screen; the result says it covers only loaded rows                 |
| [Find duplicates](editing.md#find-duplicates)                                    | Every mode                                                                  | Highlight and New tab, on the page; the filter and drop modes are off          |
| [Split into files](partition-by-column.md), Write table to database              | Yes                                                                         | Refused, as they would write one page                                          |
| [Save As](saving.md)                                                             | Writes the loaded rows, and asks first while the file is only partly loaded | Writes the whole file, or what its filters keep, as CSV, TSV, Parquet or JSON  |
| Header row suggestion banner                                                     | Yes                                                                         | No                                                                             |
| [Assistant](chatbot.md)                                                          | Sees the loaded table                                                       | Told the file's real row count; a tool asked for "the open tab" reads the file |
| Memory                                                                           | Grows with the rows loaded                                                  | About one page                                                                 |

**To work with every row** of a large file, use the [SQL panel](sql.md):
on a large-file tab its `data` is the whole file, scanned where it lies
rather than copied, so `SELECT state, count(*) FROM data GROUP BY state`
counts every row, and the result can be written out as a new file. A
query over the whole file takes about as long as reading the file once
(a second or two for a 5 GB CSV already in the disk cache); it runs in
the background, and **Cancel** stops it. A query that *returns* millions of rows is collected
in full before its first page shows, so give it a `LIMIT` or an
aggregate. On the command line, `octa --sql FILE --query '...' --stream`
does the same.

## Big files opened the ordinary way

A file below the large-file threshold opens the ordinary way, however
many rows it has. For any file over 8 MB, the reading and the preparation
that follows (whitespace trim, header cleaning, numbers, and dates when
switched on) run in the background, so the window keeps responding while
the **Loading file...** spinner turns. One setting changes that picture,
and it is off by default. Ticking it opens a warning saying what follows.

**Settings > File-Specific > Detect dates on load.** Every text column is
checked for dates before the file shows. A column is ruled out on its
first thousand values, but every value of a column that looks like dates
is read and converted: about three seconds per date column at five
million rows, added to the opening time. The window keeps responding.

- Leave it off for large files, and run **Columns > Detect dates** on the
  table that needs it. That runs in the background too, and one
  <kbd>Ctrl+Z</kbd> undoes it.

**Settings > Table View > Cell line breaks** cost little on a big table:
a row's height is measured when it first comes into view, and until then
it counts as one line. On rows of several lines the scrollbar therefore
settles as you scroll.

### Also slow on millions of rows

These run in the background, so the window keeps responding, but they
take a while and keep the tab read-only meanwhile:

- **Opening the [SQL panel](sql.md)** copies the loaded rows into the
  panel's database, about a second per million rows; a **Run** copies
  them again only when the table changed since. In large-file mode
  `data` reads the file in place and nothing is copied.
- **Answering the date or number question** converts the column, about a
  second per five million rows.
- **Save and Save As** of a big table; see
  [Saving a big table](saving.md#saving-a-big-table).

The memory figure in the tab name's tooltip is worked out from a sample
of at most 10,000 rows, so hovering a tab costs nothing at any size.

## Saving a large file under another name

**File > Save As** on a large-file tab writes the **whole file**, not the
page on screen, in the order the view is sorted by. When a search or a
column filter is active, Save As can write just the rows it keeps. The
rows never pass through Octa: DuckDB reads the file and writes the new
one, in the background, with the status bar naming the file. The new
file appears only once it is complete.

The target can be **CSV, TSV, Parquet, JSON or JSON Lines**, the formats
DuckDB writes itself; any other extension is refused with a message.
Save As cannot write over the file the tab is reading. **Save** does
nothing on a large-file tab, as nothing in it can be changed.

## When it happens

Octa decides for you, when you open the file normally. There is no
separate command for it: the mode kicks in when a file is at least
**2 GB**. A Parquet file is measured by the larger of its size on disk
and the size of its data once decompressed, which its footer states
without being read, so a small file that expands to gigabytes is caught
too. A CSV states nothing, so its size on disk is what counts.

The row count never decides on its own: an ordinary open loads the first
rows and streams the rest in as you scroll. The threshold is in
**Settings -> Performance**.

By default a dialog explains what the mode does and offers to open the
file the ordinary way instead. Tick **Do not show this again** to skip
straight into the mode in future, or turn the question back on in
Settings.

## Conversion

Parquet, CSV, TSV and JSON are read where they lie. Any other format
has to be converted to a temporary Parquet file first, and Octa says so
before it starts. That costs about what opening the file normally costs,
plus similar free disk space, and it can be cancelled while it runs.

## What the tab looks like

A banner says the tab is in large-file mode. The row numbers are the
file's real positions, so scrolling into the middle of a 40 million row
file shows row 20,000,001 rather than row 1 of a page.

Sorting a column re-asks the file for that column in order. Typing in
the search box becomes a condition applied to the file, once the typing
settles: re-running a count over a file this size on every keystroke
would lock the window.

**Column Filter** and **Filter by value** work the same way: the ticked
values become part of that condition, ANDed with the search, so they keep
every matching row of the file, exactly the rows an ordinary tab would
keep. Their value lists are counted over the whole file too, in the
background, so a value that never appears on the page can still be
ticked; the most common values are listed and typing searches all of
them. The comparisons the **Ask** box makes (`amount > 1000`) join the
same condition and keep exactly the rows they would keep in an ordinary
tab. A column holding lists, structs or binary data cannot be matched in
a query; its filter is left out, and the status bar says so.

**Nothing here holds up the window.** Opening the file (which counts its
rows), every page while you scroll or jump, and every new sort or filter
are read on a background thread: **Reading the file...** shows in the
status bar and the rows on screen stay until the new ones arrive, about a
second or two for a 5 GB file in the disk cache. While one page is being
read, the newest request waits for it, so dragging the scrollbar reads
one page at a time rather than one per frame.

That condition honours the search bar's mode, its `Aa` and whole-word
toggles and its column scope, so a wildcard or regex search here matches
the same rows it would match in an ordinary tab. An expression the regex
engine refuses matches nothing, exactly as it does in an ordinary tab.

## Command line

`--sql` can scan a file in place instead of loading it:

```bash
octa --sql huge.parquet --query 'SELECT count(*) FROM data' --stream
```

With `--stream` the file is registered as a view named `data`, so an
aggregate covers every row no matter what `--rows` says. Every other
action needs the rows themselves and says so on stderr rather than
quietly ignoring the flag.

- Command line: [`octa --sql`](../cli/sql.md)

## MCP and the assistant

Four MCP tools answer straight from a large file instead of loading it:
`count_rows` and `schema` read no rows at all, `read_table` fetches only
the rows it returns, and `run_sql` registers the file as a view so an
aggregate covers every row. Those responses carry `streamed: true`.
Tools that genuinely need the rows are untouched, so nothing quietly
answers from a slice. See the [MCP reference](../mcp/index.md).

The in-app assistant is told that the tab is a **window**, not the table.
A large-file tab holds one 2,000-row page of the file, so the tab summary
carries the file's real row count plus a note saying which rows are on
screen, and a tool asked for "the open tab" reads the file from disk
instead of the page. Without that, **Explain this file** would describe
2,000 rows of a billion-row file and say nothing about it being a slice.

## Why it is read-only

Three reasons stack up, and none of them has a partial answer:

- **The rows on screen are borrowed.** They are one page fetched by a
  query. Scroll, and that page is replaced wholesale, so an edit written
  into it would vanish the moment the window moved.
- **There is no row identity.** A SQLite or DuckDB tab can be edited
  because each row carries a `rowid`, so saving becomes an `UPDATE ...
  WHERE rowid = ?`. A Parquet or CSV scan has no such column at all, so
  there is nothing to say which row of the file an edit belongs to.
- **The files cannot be patched in place.** Parquet is columnar and
  compressed per row group; changing one cell means rewriting the file.
  In a CSV, changing a field's length shifts every byte after it.

To change a large file, query it in the [SQL panel](sql.md) on the tab,
where `data` is the whole file, and write the result out as a new file,
or use `octa --sql ... --stream` from the command line.

## Limits

- **Read-only.** There is no partial editing.
- The scrollbar spans the whole file, so dragging it lands anywhere in
  it directly, and jump-to-first-row / jump-to-last-row mean the file's
  first and last row. The mouse wheel moves within the loaded window and
  steps it a page at a time at the edges.
- Pages are fetched in the background. That is quick for Parquet, where
  a skip lands directly on the right row group, and slower for a very
  large CSV, where the file has to be walked to reach a deep offset; the
  window keeps responding either way.
- Only Parquet, CSV, TSV and JSON are read in place; everything else is
  converted first.
