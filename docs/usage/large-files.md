# Large Files

Large-file mode opens a file **read-only**, keeping its rows on disk and
fetching only what is on screen. That is the trade: you reach every row
of a file far bigger than memory, and in exchange the tab cannot be
edited.

Everything that changes data is off in this mode: editing cells, undo,
colour marks, conditional formatting, validation highlighting, find and
replace. Scrolling, sorting, filtering, search, [Summary](summary.md),
[Chart](chart.md), the [SQL panel](sql.md), export and convert all work.

## When it happens

Octa decides for you, when you open the file normally. There is no
separate command for it: the mode kicks in when a file is at least
**10 GB**, or when it states more rows than Octa would load anyway
without being read (Parquet says so in its footer; a CSV cannot).

That row threshold is the **maximum rows loaded on open** setting you
already have, not a second one to keep in step. Set it to unlimited and
only the file size decides. Both settings are in
**Settings -> Performance**.

By default a dialog explains what the mode does and offers to open the
file the ordinary way instead. Tick **Do not show this again** to skip
straight into the mode in future, or turn the question back on in
Settings.

<!-- TODO screenshot: the large-file notice with its two open buttons.
     Listed in docs/assets/screenshots/INDEX.md. -->

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

## From the command line

`--sql` can scan a file in place instead of loading it:

```bash
octa --sql huge.parquet --query 'SELECT count(*) FROM data' --stream
```

With `--stream` the file is registered as a view named `data`, so an
aggregate covers every row no matter what `--rows` says. Every other
action needs the rows themselves and says so on stderr rather than
quietly ignoring the flag.

## For the assistant

Four MCP tools answer straight from a large file instead of loading it:
`count_rows` and `schema` read no rows at all, `read_table` fetches only
the rows it returns, and `run_sql` registers the file as a view so an
aggregate covers every row. Those responses carry `streamed: true`.
Tools that genuinely need the rows are untouched, so nothing quietly
answers from a slice. See the [MCP reference](../mcp/index.md).

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

To change a large file, use the [SQL panel](sql.md) on the tab and write
the result out as a new file, or `octa --sql ... --stream` from the
command line.

## Limits

- **Read-only.** There is no partial editing.
- The scrollbar spans the whole file, so dragging it lands anywhere in
  it directly, and jump-to-first-row / jump-to-last-row mean the file's
  first and last row. The mouse wheel moves within the loaded window and
  steps it a page at a time at the edges.
- Pages are fetched on the interface thread. That is comfortable for
  Parquet, where a skip lands directly on the right row group, and
  slower for a very large CSV, where the file has to be walked to reach
  a deep offset.
- Only Parquet, CSV, TSV and JSON are read in place; everything else is
  converted first.
