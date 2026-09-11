//! The view modes and the special readers: compare, chart, map, record,
//! SQL, EPUB, archives, large files, compressed files and datasets.
//!
//! One of six topic files split out of `content.rs`, which held all 77 section
//! bodies in a single 4,261-line, 192 KB file. Text moved verbatim; the parent
//! `content/mod.rs` re-exports every constant, so `documentation::sections()`
//! is untouched.
//!
//! ASCII only: egui's bundled font renders typographic punctuation as tofu.

pub const VIEW_MODES: &str = r#"# View Modes

Switch via the **View** menu. Only modes applicable to the current file are
enabled.

- **Table View** (default): structured tabular display with sorting,
  filtering, and editing.
- **Raw Text**: shows the file content as plain text. For CSV/TSV the toolbar
  exposes Quote / Escape / Delimiter combos and an **Align Columns** toggle
  with per-column colouring. For JSON it exposes a **Format JSON** toggle that
  breaks a minified file into indented lines: only the whitespace between
  tokens changes, so numbers keep their exact digits, text keeps its escapes
  and the keys stay in file order. It is a way of reading the file rather than
  an edit of it, so the tab is not marked changed and un-ticking puts the
  on-disk text back. Syntect-based syntax highlighting kicks in for
  source-code extensions (Python, Rust, shell, Terraform, ...) and also for
  JSON, YAML, XML and TOML files; the size cap is configurable under
  **Settings -> Performance**. Dragging a selection to the edge of the view
  keeps scrolling, so a selection can run past the lines on screen (the
  Markdown and SQL editors do the same).
- **Markdown View**: rendered markdown for `.md` files. Files open in
  **Preview** mode by default (rendered output only). A toolbar toggle
  switches between Preview / Split / Edit. Split places a TextEdit beside the
  preview for live editing. Links in the preview open in your system browser.
  The preview follows the app's body text size, so the **Font size** setting
  and Ctrl+Plus / Ctrl+Minus zoom scale the rendered document too.
- **Notebook View**: rendered Jupyter notebook with cell outputs. Code cells
  use syntect highlighting.
- **JSON Tree** / **YAML Tree**: collapsible tree view for JSON / JSONL /
  YAML. Keys are renamable, values editable, and you can add keys to objects
  in place.
- **EPUB Reader**: chapter-by-chapter reading view for `.epub` files. See
  the **EPUB Reader** section for details.
- **Map View**: slippy-map view for `.geojson` files. See the **Map View**
  section for details.
- **Compare View**: side-by-side comparison of two files. See the
  **Compare View** section for details.

The **Cycle view mode** shortcut (default **F4**, remappable) advances through
the modes available for the current tab. **F8** toggles a session-only
read-only mode that disables every editing path while still allowing copy
and Save-As.

## Default view per file type

Some files open in a non-Table view that suits them better: a `.json`
file opens in the JSON Tree, and a `.yml` / `.yaml` file opens in Raw
Text. You can always switch to another mode from the View menu; this
just picks a sensible starting point. JSONL and every other format
still open in Table View.

## Open as... (a file with a misleading extension)

Which views a file offers depends on how it was parsed, and Octa parses it
by extension. A `.log` file that actually contains JSON is read as plain
text, so the JSON Tree is not on offer.

Two entries fix that, depending on whether the file is open yet:

- **File > Open as...** for a file you have not opened. Pick the format,
  then pick one or more files. The file dialog is deliberately unfiltered
  (every file is shown), since these are exactly the files whose extension
  Octa would otherwise route somewhere unhelpful. Each opens in its own tab.
- **View > Reopen as** for the file already in the current tab, which is
  re-read in place.

Both offer JSON, JSON Lines, CSV, TSV, YAML, TOML, XML, Markdown, and Plain
text. Pick JSON for that `.log` and it parses as JSON, tree view and all,
exactly as if the file had been named `.json`. Log files holding one JSON
object per line want **JSON Lines** instead.

Nothing on disk is renamed or rewritten: this only changes how Octa reads
the file. Reopening re-reads from disk, so unsaved edits in that tab are
discarded. If the content does not parse as the chosen format, the tab is
left as it was and the status bar reports the error.
"#;

pub const COMPARE_VIEW: &str = r#"# Compare View

Compare two files side-by-side. Triggered in four ways:

- **View -> Compare with...**: opens a file picker; the active tab is the
  left side, the picked file is the right.
- **View -> Compare with git version...**: compare the current file (with
  any uncommitted changes) against a committed version from git. Opens a
  small dialog defaulting to **HEAD** (the last commit) with a dropdown of
  recent commits that touched the file, so you can pick any older revision.
  The dialog also has **Open in new tab**, which loads that past version on
  its own instead of comparing. Works for any tracked file, text or binary
  (the committed bytes are read straight from git). Requires the file to be
  saved inside a git repository; otherwise a status message says so.
- **Right-click a tab -> Compare with active tab**.
- The **Compare selected tabs** shortcut (default **F9**, remappable) when
  exactly one tab is **Ctrl-clicked** as the right side.

Four sub-modes toggle in the Compare toolbar:

- **Text Diff**: git-style line-by-line diff of the raw text content,
  rendered with `+` / `-` / `~` markers. Has a 500 ms timeout against
  pathologically slow inputs.
- **Row Hash Diff**: hash the user-picked columns per row (BLAKE3, fast
  and stable). Rows bucket into **Left-only**, **Right-only**, **Shared**.
  Each bucket is expandable and shows the actual cell content (capped at
  50 rows displayed per bucket). With no columns picked, every column is
  hashed; only the first 8 columns are shown to keep rendering snappy.
- **Ordered**: positional row-by-row comparison. Row 1 on the left is
  compared with row 1 on the right, row 2 with row 2, and so on, naming
  exactly which columns differ in each row. Rows past the end of the
  shorter table are reported as only-on-one-side. Use this when both
  files are in the same order and you want a cell-level diff.
- **Join (by key)**: match rows by one or more **key columns** you tick
  (e.g. an ID column), then report which rows were added, which were
  removed, and which changed - listing the changed columns for each pair.
  The same key column name must exist on both sides. This is the
  "same record, what changed?" comparison, regardless of row order.

The Ordered and Join modes share the exact logic used by the command-line
`octa --diff` and the assistant's diff tool, so all three agree. Their
result is shown as one table: a **status** column (`only_in_a`,
`only_in_b`, `changed_a`, `changed_b`), a **changed_columns** column, and
the data columns. Cross-format comparison works throughout because only the
textual representation of each cell is compared.

## Copying

In **Text Diff** the text is selectable: drag to mark, double-click a word,
or triple-click a line, then copy with **Ctrl+C** or right-click **Copy
selection**. The right-click menu also offers **Copy left side**, **Copy
right side**, and **Copy as unified diff** for the whole comparison. Long
lines scroll sideways within each pane rather than wrapping, so the line
numbers stay aligned.

Row Hash Diff, Ordered, and Join offer **Copy table** (Ctrl+C or right-click)
for the visible result.
"#;

pub const EPUB_VIEW: &str = r#"# EPUB Reader

When you open a `.epub` file, the EPUB Reader is the default view. The
top toolbar shows:

- The **book title** (from `<dc:title>`).
- **Previous** / **Next** buttons to step through chapters.
- A **chapter combo** showing the full chapter list; pick any chapter
  to jump straight to it.

The chapter body renders through the same Markdown pipeline as the
Markdown view (the chapter's XHTML is converted to Markdown at load
time). Embedded images appear as a thumbnail strip beneath the chapter
text.

The flat **Table** view is still available (one row per paragraph with
`chapter`, `paragraph`, and `text` columns) and can be searched / filtered
like any other tabular file.
"#;

pub const MAP_VIEW: &str = r#"# Map View

For `.geojson` and `.shp` (Shapefile) files. The Map view is the
default; the Table view is still available with one row per feature, a
`__geometry` column holding the WKT representation, and one column per
property. Shapefiles read geometry from the `.shp` and attribute columns
from the sibling `.dbf`.

You can also plot **any** table that has latitude/longitude columns:
open a CSV/Parquet/Excel file with columns named `lat`/`latitude` and
`lon`/`lng`/`long`/`longitude` (numeric, in range) and **View -> Map**
becomes available, drawing one point per row. The Map toolbar shows
**Lat** / **Lon** dropdowns to correct the column choice; the points
update live.

Top toolbar:

- Feature count.
- **Tiles** / **Geometry only** radio. Tiles fetches a slippy map from
  the configured tile URL (default OSM). Geometry-only paints the
  shapes on a blank canvas; useful offline or to focus on the data.
- **Reset view**: re-centres on the feature centroid and resets zoom.

Interaction:

- **Scroll wheel** zooms in / out.
- **Double-click** zooms in.
- **Click-drag** pans.

The tile URL template, default mode, and "fall back to geometry on tile
fetch failure" toggle live under **Settings -> Map**. For production
deployments please honour the
[OSM tile-usage policy](https://operations.osmfoundation.org/policies/tiles/)
or point at a self-hosted or commercial tile provider.
"#;

pub const RECORD_VIEW: &str = r#"# Record View

One row at a time, shown vertically as a list of field name / value
pairs. For tables too wide to read in the grid, where reading a single
row means scrolling sideways past forty columns.

Reach it via **View -> Record View**, or cycle to it with **F4**. It is
offered for any tab that has columns.

- The `<` and `>` buttons step to the previous / next row, and grey out
  at the ends. The **Up** and **Down** arrow keys do the same.
- Navigation walks the **visible** rows, so an active search or column
  filter narrows what you step through. The counter reads
  "Row 3 of 128" against the filtered set, not the whole file.
- Filtering away the row you were on lands you on the first row still
  visible rather than an empty pane.
- Search matches are highlighted in the values, same as everywhere else.
- **Click a value to edit it**, then press Enter or click away to
  commit. Edits go into the real row through the same overlay the grid
  uses, so undo/redo, the modified marker and Save all behave normally.
  In read-only mode (**F8**) values are not clickable.

The record view and the table view share one selection, so switching
between them keeps your place in both directions.
"#;

pub const CHART_VIEW: &str = r#"# Chart

Plot the active table as a histogram, bar, line, scatter, or box chart.
The chart opens as its own **tab** -- not a mode of the source tab --
so you can have several charts of the same data running at once.

Trigger via **Analyse > Chart...** or **F5** (remappable). The entry is
hidden on string-only tables since there's nothing to plot.

## Chart kinds

The leftmost combo in the control bar picks the chart kind:

- **Histogram**: numeric / Date / DateTime X, no Y. Frequency count,
  binned via Sturges' rule by default (untick **Auto (Sturges)** to
  set the bin count by hand).
- **Bar**: categorical or numeric X, one or more numeric Y. Groups
  rows by X and aggregates Y(s) via the **Agg:** picker
  (Sum / Avg / Count / Min / Max). Caps at `chart_max_categories`
  (default 200) distinct categories.
- **Line**: numeric / Date / DateTime X, one or more numeric Y. One
  polyline per Y column. Points are auto-sorted by X.
- **Scatter**: numeric / Date / DateTime X, one or more numeric Y.
  Disconnected points.
- **Box**: one or more numeric Y, no X. Tukey 5-number summary per
  Y column (whiskers extend to the actual values within 1.5 * IQR).

## Dates on the axes

Date columns chart as "days since 1970-01-01", DateTime columns as
"seconds since the Unix epoch". The parser accepts ISO, dotted
European, slashed European, and slashed US date formats; for
timestamps add the time component with optional fractional seconds
and an optional trailing `Z`.

## Bar charts: categorical X axes

Bar charts with a string X column (e.g. country codes) show each
category as its own tick with the category name as its label -- not
a numeric index. Categories appear in first-seen order so the X
axis matches the source table.

## Customise

The **Customise** collapsible exposes:

- **Title**: free text rendered above the plot.
- **X-axis label** / **Y-axis label**: override the column-derived
  defaults.
- **Legend**: Off / Top-left / Top-right / Bottom-left / Bottom-right.
- **Grid**: tick to draw the background grid lines, untick for a
  clean plot area.
- **Series**: per-Y-column **Label** override (used in the legend +
  tooltip) and a custom **Colour** picker.

### Y axis

- **Min / Max**: force fixed bounds (both must be set).
- **Step**: custom grid step in original-data units.
- **Integers only**: format Y ticks as whole numbers.
- **Log scale**: apply log10 to Y before plotting; non-positive
  values are dropped, axis label gets a `(log10)` suffix.

## Exporting

Three buttons sit on the right of the row above the plot:

- **Export PDF**: one-page vector PDF (via `svg2pdf`).
- **Export PNG**: 2x retina-resolution raster PNG (1600 x 1000 px).
- **Export SVG**: the hand-emitted SVG itself.

All three formats are derived from the same SVG and look identical
regardless of window size or DPI.

## Sampling

Above **Settings > Performance > Chart max points** (default 100,000),
Histogram / Line / Scatter evenly-spaced downsample. Bar and Box
always work off the full input.

## Interacting

- **Drag** pans.
- **Mouse wheel** zooms.
- **Right-drag a box** zooms into that region.
- **Double-click** resets to auto-bounds.
- **Hover** a point or bar to see its coordinates in a tooltip.
"#;

pub const SQL_VIEW: &str = r#"# SQL View

The **SQL Query** view exposes the active table to an in-memory DuckDB
connection as a temp table named `data`. Press **Ctrl+Enter** to run the
query under the cursor.

- The editor has line numbers, syntax-aware case conversion (UPPER / lower)
  via right-click, and a chip-style autocomplete row showing matching column
  names and SQL keywords. Disable autocomplete in
  **Settings > SQL > Autocomplete** (on by default).
- Results render under the editor with a **row counter** above the grid,
  followed by how long the query took in brackets (`1234 result rows
  (84 ms)`, seconds past one second; a failed query is timed too)
  (display-only; it is never part of the data or an export); errors render
  in red.
- Results honour the initial-load row cap (**Settings > Performance**,
  default 5,000,000): a bigger SELECT stops there instead of exhausting
  memory, and the counter notes "row cap reached". Applies to local DuckDB
  and to queries run on a live database connection alike.
- **Ctrl+Shift+E** (default) exports the current SQL result.
- The panel can be docked Bottom (default), Top, Left, or Right via
  **Settings > SQL > Panel position**.

## Ask

Type what you want in plain words ("revenue per country, biggest first")
and Octa writes the SQL into the editor at your cursor. It is never run
for you: read it, change it if you like, then press Run. Only a single
SELECT is ever produced, so Ask cannot hand you a statement that changes
data.

A dropdown beside the box picks which assistant answers, the same
control the search bar's Ask has. It is per tab and starts on whatever
the chat panel is set to, so one question can go to a bigger model
without changing the panel; with only one profile configured there is
no choice to make and the dropdown stays away.

Ask sends the active table's column names, their types and the row count
to the chat profile you have configured. It does not send the data
itself. When the panel is set to run on a server, the query is written
in that database's dialect against the real table name.

On a database tab set to run on the server, Ask also looks up the
foreign keys the database declares for that table and sends the tables
one hop away, with their column names and the key each join uses. It
looks in the table's own schema, so a related table parked in a
different schema is named without its columns. That
is what lets it answer "who spent the most" with a join instead of
guessing. It reads the catalogue only, never any rows, and only joins
the pairs the server declared. Databases that accept foreign keys
without enforcing them (Redshift, Snowflake, Databricks, BigQuery) often
declare none, and a catalogue you may not read is skipped in silence: in
either case Ask asks about the one table, as before. A local (DuckDB)
query never gets this, because the neighbouring tables are not there to
join.

A local query gets the **workspace** instead: every table registered
from another tab and every attachment's tables, with their columns, so
Ask can write a join across them and call each one by its real name.
The list is capped at 12 tables and 40 columns apiece to keep the
request small, and attachment tables come from the cached listing, so
no extra query goes to the server. On a server tab the row count sent
is the table's own, not the size of the page currently on screen.

Ask is greyed out when no chat profile is set up, or when there is
nothing for the prompt to describe: no columns on the tab **and** no
registered or attached workspace tables. An empty tab with a database
attached is therefore fine, which is exactly the tab the panel exists
for. Hover it to see which case applies.

The search bar has a sibling Ask toggle that produces filters rather than
a query, under the same one-request rule.

## Workspace

Each tab owns a persistent SQL **workspace** that outlives individual
runs. The collapsible Workspace section above the editor lists what is
queryable:

- `data` - the active table. SQL sees a snapshot; after editing cells,
  click **refresh** next to `data` to push your edits into the workspace.
- **+ Add table...** loads more files as extra tables (any readable
  format), for cross-file JOINs.
- **Attach database...** ATTACHes a DuckDB or SQLite file
  (`alias.schema.table`).
- **Attach connection** ATTACHes a saved live-database connection
  (PostgreSQL / MySQL read-only via DuckDB extensions; SQL Server,
  Oracle and the warehouses have no extension, so their tables come in
  as plain workspace tables and their entry opens into the server's
  tree). A native attachment's alias
  is the connection name lowercased with punctuation as `_`; the
  **Attached connections** box next to the Inspector lists each alias
  with a one-click example query.
- **Attach cloud** picks objects out of a saved cloud connection and
  registers each as a workspace table.

**Picking what to attach.** PostgreSQL, MySQL and Redshift attach
natively: one click takes the whole server and nothing is copied. Every
other engine has no DuckDB extension, so an attach *fetches* the tables
it covers - which is why its entry opens into the tree instead. Walk
down to a single table (the usual answer), stop on a schema, or click
**Attach everything here** to take the level you are standing on.
Trino, Snowflake, Databricks and BigQuery put a catalog above the
schema, so their tree starts there: "the schemas of this server" is not
a question they can answer, and their root offers no Attach everything
here. An imported table is named after **itself** (`orders`), joins the
workspace as an ordinary table rather than an attachment, and carries
where it came from as its origin (hover the row). A name already in use
takes a `_2` suffix, and **double-clicking a name renames it**, so a
`FROM` clause says whatever you want; `data` is the one name that
cannot change. An import is capped at 60
tables per attach; over that nothing is attached at all and the message
says so, so no table is ever silently left out. Drill one level deeper,
or query the server directly with Run on server.

**Cloud objects as workspace tables.** Attach cloud lists your saved
cloud connections and opens a picker on the one you choose. Browse the
folders, tick what you want, and Add to workspace downloads each object
and registers it as a table named after the file. Only files a reader
can open are offered. Several ticks give several tables; **Combine the
picked objects into one table** unions them into one instead, and starts
off on purpose - a union reconciles differing schemas, and Octa does not
do that to your data unless you ask.

Clicking any table shows it in the **Inspector** (columns, sample rows,
Copy / Insert / Run buttons). The SQL panel also opens on an **empty
tab** (Analyse > SQL): attach connections and query servers without
opening a file - there is just no `data` table then.

## History and snippets

The SQL toolbar offers two ways to reuse queries:

- **History** lists the queries you have actually run, most recent first,
  each with how long it took and how many rows came back. Pick one to load
  it into the editor, or use **Clear history** to forget the lot. It is
  scoped and kept between sessions: a database tab records against its
  connection and a file workspace against its file, so production queries
  do not turn up while you are poking at a CSV. **Settings > Databases >
  Query history** switches it off or changes how many are kept (20 by
  default, 0 keeps all). Turning it off also deletes what was kept, since
  a query can carry values out of your data.
- **Snippets** opens a manager window for a saved library of named queries
  that persists across sessions. Use **Save current query as snippet...**
  to store the editor content under a **name** and an optional
  **description**; each snippet has **Insert** (load it into the editor)
  and **x** (delete). The window has minimise / maximise / close controls
  and is resizable. Snippets live in `sql_snippets.json` in Octa's config
  directory.

## CREATE TABLE opens a new tab

A `CREATE TABLE` or `CREATE VIEW` statement turns into a new tab named after
the table, with the declared columns and types (`CREATE TABLE people (id
INTEGER, name VARCHAR)` gives an empty grid to type into; `CREATE TABLE top
AS SELECT * FROM data ORDER BY score DESC LIMIT 10` gives a tab with those
rows). The tab you ran it from is untouched, so this works from an empty tab
with nothing open, and it is not blocked by read-only mode: a new tab is not
an edit. The table is handed over rather than kept, so `x` is not queryable
from the original panel afterwards; switch to the new tab and it is `data`
there. A table created inside an attached database is left alone (and
attachments are read-only, so DuckDB refuses it anyway).

## Mutation highlight

After a mutation query (`INSERT` / `UPDATE` / `DELETE`) that changes the
table, Octa briefly marks the changed cells and any new rows in green so
you can see what the query did. Turn this off, or change how long it stays
(in seconds), under **Settings -> SQL** (**Highlight SQL changes** /
**Highlight duration**). The marks clear themselves automatically.

The workspace's DuckDB connection is per tab and persistent: added
tables and attachments stay across runs and are dropped when the tab
closes. Mutations change the in-memory table only; save the file to
persist them.
"#;

pub const ARCHIVE_VIEWER: &str = r#"# Archive Viewer

Open `.zip`, `.tar`, or `.tgz` files to see their contents listed as
a regular table.

Columns: `path`, `size_bytes`, `compressed_bytes` (null for tar),
`mtime`, `is_dir`, `type` (file extension hint).

## Opening an entry

An action bar above the table shows when the active tab is an
archive. Select any row and click **Open selected entry**. The entry
is extracted into a tempfile and opened as a new tab via the normal
file-open path -- every format reader Octa knows about works (CSV,
JSON, Parquet, ...).

Directory rows can't be opened (the button is greyed for them).
The tempfile lives until the OS cleans /tmp.

## Supported / unsupported

Supported extensions: .zip, .tar, .tgz.

Not auto-routed: .tar.gz (would collide with .csv.gz etc). Rename to
.tgz or open via "All files" in the picker. .tar.bz2 and .7z aren't
supported.

The reader is read-only -- there is no "save to archive" gesture.
"#;

pub const PARSE_IN_NEW_TAB: &str = r#"# Parse in New Tab

Take part of the table you are looking at and re-read it as a different
format, in its own tab. Useful when a cell holds a JSON document, a
column holds YAML fragments, or you want to see the whole table as
Markdown without writing a file first.

**Edit > Parse in new tab** offers four scopes:

- **Cell**: the cell under the cursor, verbatim.
- **Row**: the selected row.
- **Column**: the selected column.
- **Whole table**: everything.

Then pick the format to parse it AS: JSON, JSON Lines, YAML, TOML, XML,
CSV, TSV, Markdown or Plain Text. The list stops there on purpose -
parsing arbitrary cell content as Parquet or Excel would need binary
bytes and produce noise.

## What actually happens

Cell, Row and Column build a small table of their own first, keeping the
source column names as headers, and that is what gets written out and
re-read. So a row parsed as JSON comes back as an object with your
column names as keys, not as a bare list of values. Plain Text is the
exception: it is passed through verbatim.

The result is written to a temporary file and opened through the normal
reader for the format you picked, so it behaves like any other tab -
same view modes, same SQL panel, same export. Its **source path is
cleared**, so saving asks where to put it and can never overwrite the
file you started from.

## When the parse fails

If the text is not valid in the format you chose, the tab opens in the
Raw view with a banner saying so, rather than showing nothing. That is
usually the fastest way to see which line is malformed.
"#;

pub const LARGE_FILES: &str = r#"# Large Files

Large-file mode opens a file read-only, keeps its rows on disk and fetches only
what is on screen. That is the whole trade: every row of a file far bigger than
memory is reachable, and in exchange the tab cannot be edited. Editing cells,
undo, colour marks, conditional formatting, validation highlighting and find
and replace are all off. Scrolling, sorting, filtering, search, Summary, Chart,
the SQL panel, export and convert all work.

**When it happens.** Octa decides for you, when you open the file the ordinary
way. There is no separate command for it. The mode kicks in for files of at
least 10 GB, or for files that state more rows than Octa would load anyway
without being read (Parquet says so in its footer; a CSV cannot). That row
threshold is the **maximum rows loaded on open** setting you already have, not
a second one to keep in step: set it to unlimited and only the file size
decides. Both live under **Settings > Performance**. A dialog explains the
trade and offers to open the file the ordinary way instead; tick **Do not show
this again** to skip it in future, and turn the question back on in Settings.

**Conversion.** Parquet, CSV, TSV and JSON are read where they lie. Any other
format is converted to a temporary Parquet file first, which costs about what
opening it normally costs plus similar free disk space, and can be cancelled
while it runs. The dialog says which case you are in before you commit.

**What the tab looks like.** A banner says the mode is on. Row numbers are the
file's real positions, so scrolling into the middle of a forty million row file
shows row 20,000,001. Sorting a column asks the file for that column in order.
Typing in the search box becomes a condition applied to the file, once the
typing settles: re-running a count over a file this size on every keystroke
would lock the window.

**Getting around.** The scrollbar spans the whole file, so dragging it lands
anywhere in it directly, and jump-to-first-row / jump-to-last-row mean the
file's first and last row. The mouse wheel moves within the loaded window and
steps it a page at a time at the edges.

**Why it is read-only.** Three reasons stack up. The rows on screen are one
page fetched by a query, and scrolling replaces that page wholesale, so an edit
written into it would vanish when the window moved. There is no row identity: a
SQLite or DuckDB tab is editable because every row carries a rowid, so saving
becomes an UPDATE for that rowid, and a Parquet or CSV scan has no such column
at all. And the files cannot be patched in place anyway - Parquet is columnar
and compressed per row group, so one cell means rewriting the file, and in a
CSV a longer value shifts every byte after it. To change a large file, use the
SQL panel on the tab and write the result out as a new file.

**The assistant sees the file, not the page.** A large-file tab holds one
2,000-row page, so the assistant is told the file's real row count plus which
rows are on screen, and a tool asked for "the open tab" reads the file from
disk instead of the page. Without that, **Explain this file** would describe
2,000 rows of a billion-row file and never say it was looking at a slice.

**Ceilings.** Pages are fetched on the interface thread, which is comfortable
for Parquet and slower for a very large CSV, where a deep position means
walking the file.

**Elsewhere.** `octa --sql FILE --query '...' --stream` lets DuckDB scan the
file in place, so an aggregate covers every row no matter what `--rows` says.
Other actions need the rows themselves and say so rather than ignoring the
flag.
"#;

pub const COMPRESSED_FILES: &str = r#"# Compressed Files

Octa reads gzip (`.gz`) and Zstandard (`.zst`) compressed files
transparently: open `data.csv.gz` and it decompresses to a temporary file
and loads as a normal CSV. This works everywhere a file can be opened:
the GUI, the folder sidebar, the CLI actions, and the MCP tools.

- The inner format comes from the middle extension (`.csv.gz` -> CSV,
  `.json.zst` -> JSON, and so on).
- **Saving** a compressed file recompresses it back to the original
  path with the same codec. Save As to a plain extension writes
  uncompressed.
- A decompression size cap guards against decompression bombs:
  **Settings > Files > Max decompressed size** (default 4 GB, with an
  Unlimited override). Files that inflate past the cap are refused with
  a clear error.
"#;

pub const DATASETS: &str = r#"# Datasets (Folder of Parts)

Many tools write a *table* as a *folder*: Spark and friends produce
`part-00000.parquet`, `part-00001.parquet`, ...; lakehouses store Delta
Lake or Apache Iceberg directories. Octa opens all of these as one table.

- **File > Open table folder...** picks a directory. Delta Lake
  (`_delta_log/`) and Iceberg (`metadata/`) directories load through
  DuckDB's extensions (installed over the network on first use, then
  cached).
- Any other directory is scanned (up to 8 levels deep) for data parts:
  Parquet, CSV/TSV, or JSON Lines. The majority family wins, and the
  matching files are read as one table; a banner lists skipped files.
- In the folder sidebar, right-click a directory and choose **Open as
  dataset...** for the same behaviour without the picker.

The initial-load row cap applies as usual for very large datasets.
"#;
