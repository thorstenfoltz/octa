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

- **Table view** (default): structured tabular display with sorting,
  filtering, and editing.
- **Raw text**: shows the file content as plain text. For CSV/TSV the toolbar
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
  Markdown and SQL editors do the same). A **Wrap lines** tick box folds
  long lines at the edge of the pane instead of scrolling sideways to
  them. It is off by default and folds anywhere, not only at spaces,
  which is what reaches a minified line or a field holding a whole
  stringified JSON blob (Format JSON cannot break such a value: a real
  newline inside a JSON string would corrupt the document). It is
  unavailable while Align Columns is on, because folding a padded row
  destroys the alignment.
- **Several carets** (Raw, Markdown Edit and the SQL editor), like in VS
  Code: **Alt+click** adds a caret (on a caret, takes it away),
  **Shift+Alt+Down / Up** adds one on the line below or above in the same
  column, and **Shift+Alt+I** puts one at the end of every marked line.
  Typing, Backspace, Delete, Enter, Tab, the arrows,
  Home and End then act at every caret. A paste with exactly one line per
  caret gives each caret its own line; any other paste goes in whole at each.
  **Esc**, a plain click, Undo or any other key (a word jump, Shift+arrow)
  goes back to one caret. These are the defaults: the keys can be changed
  under **Settings -> Shortcuts -> Text editors**, and the click under
  **Settings -> Search & Editor -> Add a caret with** (Alt+click,
  Ctrl+click or Off). Where Alt+click moves the window (some Linux
  desktops), pick Ctrl+click or use the keys.
- **Markdown view**: rendered markdown for `.md` files. Files open in
  **Preview** mode by default (rendered output only). A toolbar toggle
  switches between Preview / Split / Edit. Split places a TextEdit beside the
  preview for live editing. Links in the preview open in your system browser.
  The preview follows the app's body text size, so the **Font size** setting
  and Ctrl+Plus / Ctrl+Minus zoom scale the rendered document too, and it
  uses the whole width of its pane, so tables spread out as the window grows.
  **File > Export to PDF...** prints the document as the preview shows it.
- **Notebook view**: a Jupyter notebook cell by cell. Code cells are editable
  and highlighted as Python; Markdown cells show their source. Outputs sit
  beneath or beside each code cell (Settings, **Notebook output layout**):
  text, and pictures (PNG, JPEG, GIF, WebP, SVG) at their own size on a
  white background. Hover a picture for the text the notebook keeps with
  it; click it to open it in the binary viewer, to see it full size, save
  it or open it in another program. HTML outputs show as their plain text.
  A notebook has no Table view: being JSON, it offers the JSON tree and the
  Raw view instead, and cells are added or deleted in the Raw view. An edit in
  one shows in the others without a save. The command line and MCP tools read
  it as a table, where a picture with no text reads as `[image/png 640x480]`.
- **JSON tree** / **YAML tree**: collapsible tree view for JSON / JSONL /
  YAML, and for Jupyter notebooks, which are JSON. Keys are renamable, values editable, and you can add keys to objects
  in place. A field whose *string* holds a whole JSON document gets a
  small `[+]`: click it to unfold that string into a tree of its own,
  keys and all, with every level opened. Display only, so the value
  itself is untouched and the unfolded rows cannot be edited, but they do
  take part in search.
- **EPUB reader**: chapter-by-chapter reading view for `.epub` files. See
  the **EPUB reader** section for details.
- **Map view**: slippy-map view for `.geojson` files. See the **Map view**
  section for details.
- **Compare View**: side-by-side comparison of two files. See the
  **Compare View** section for details.

The **Cycle view mode** shortcut (default **F4**, remappable) advances through
the modes available for the current tab. **F8** toggles a session-only
read-only mode that disables every editing path while still allowing copy
and Save-As. The entries that change data are greyed out in the right-click
menus and in the Edit, Columns and Data menus, and cut, paste, replace,
header drags and renames, Undo and Redo do nothing. Sorting, filtering,
searching and colour marks still work: they change what you see, not the
data.

## Default view per file type

Some files open in a non-Table view that suits them better: a `.json`
file opens in the JSON tree, a `.yml` / `.yaml` file in Raw Text, and a
`.ipynb` notebook in the Notebook view. You can always switch to another mode from the View menu; this
just picks a sensible starting point. JSONL and every other format
still open in Table view.

## Unsaved changes in every view

Switching views never needs a save. Change a cell in the Table view, then
switch to Raw text, the JSON or YAML tree, Markdown or the Compare text diff:
the text shows the table the way **Save** would write it. Change the text
instead and switch back: Table, Record, Chart, Timeline and Map all read the
edited text. Closing the tab without saving still drops the changes.

Save writes whichever side you changed last: the text exactly as you typed
it, or the table through the format's writer. The text view shows the table
the way Save writes it, so it can differ from your file in more places than
your change (JSON indentation, YAML comments). A save would change those too.

Two cases stop a switch, with the reason in a banner: text that no longer
reads as the file's format (you stay in the text view to fix or undo it), and
a file only partly loaded as a table, where the text holds the whole file and
the table only its first rows. Save first, then switch.

## Open as... (a file with a misleading extension)

Which views a file offers depends on how it was parsed, and Octa parses it
by extension. A `.log` file that actually contains JSON is read as plain
text, so the JSON tree is not on offer.

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

## Tolerance

Open **Tolerance** above the Ordered or Join result to count values as the
same when they are close enough, or to leave columns out of the comparison.

- **Numbers within**: two numbers count as the same when they differ by at
  most the absolute amount you type (for example 0.01), or by at most the
  percentage you type of the larger one. Either is enough. Both decimal
  marks work. Empty means exact.
- **Times within**: dates and times count as the same when they are at most
  this many seconds apart. A date counts as midnight.
- **Ignore upper/lower case** and **Ignore extra spaces**: text compares
  without regard to case, or with spaces at the ends removed and runs of
  spaces counted as one. These also apply when rows are matched on the key
  columns.
- **Leave out these columns**: ticked columns are not compared, but they
  still appear in the result.

Tolerance never pairs keys. When some cells differ only within tolerance,
the summary adds "N cells differ only within tolerance". It does not apply
to Row Hash Diff, which matches rows by fingerprint.

## Editing in Text Diff

The left pane is the file you have open, so it is a real editor: type in it
and the tab changes exactly as if you had edited it in Raw text. **Ctrl+S**
saves, the tab shows the usual unsaved marker, and the diff re-computes as
you type. Read what changed against the committed version and fix it without
leaving the comparison.

The right pane is a committed revision or a second file, with nothing to
write back to, so it stays read-only: selectable and copyable, and
keystrokes do nothing.

It is the same editor as Raw text: **Tab** indents in place by the **Tab
size** setting instead of jumping to the next control, and dragging a
selection past the edge of the pane scrolls it. Only the tab you type
becomes spaces; tabs the file already contained are left alone.

The left pane carries blank filler rows wherever the right side has a line
of its own, so the two stay aligned while you read. They are not part of
your file: type on one and it becomes a real line, leave it alone and it
never reaches the file. The pane is read-only whenever the tab is, which
covers read-only mode (**F8**), a large-file tab and a database tab without
write access; hovering **Left:** in the toolbar says which applies.

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

pub const EPUB_VIEW: &str = r#"# EPUB reader

When you open a `.epub` file, the EPUB reader is the default view. The
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

pub const MAP_VIEW: &str = r#"# Map view

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

pub const RECORD_VIEW: &str = r#"# Record view

One row at a time, shown vertically as a list of field name / value
pairs. For tables too wide to read in the grid, where reading a single
row means scrolling sideways past forty columns.

Reach it via **View -> Record view**, or cycle to it with **F4**. It is
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

pub const TIMELINE_VIEW: &str = r#"# Timeline

Room bookings, shifts, holidays, projects: any table where a row has a start
and an end. **View -> Timeline** draws each row as a bar on a time axis and
outlines the bars that overlap. Offered for any table with a date or date and
time column; **F4** cycles to it.

**Columns.** **Start** and **End** pick the first two date columns by
themselves; End **(none)** turns every row into a point. **Label** is the text
written on each bar where it fits and shown when you hover one. **Lane** groups the bars into bands, one per value
(a room, a person); overlaps are only looked for inside a lane.

**Overlaps.** Bars in the same lane that share time are outlined in the
warning colour and stacked on their own track so neither hides the other. Bars
that only touch (one ends at 11:00, the next starts at 11:00) do not overlap. A
point overlaps a bar it falls strictly inside. **Open overlaps...** opens a new
tab with one row per pair: the lane, and the row, label, start and end of both,
named after your columns. `_a` is the row that starts first, `_b` the one that
starts while `_a` is still running; hover a header for what it holds.

**Rows not drawn.** A row that ends before it starts is counted in the line
above the timeline (hover it for the row numbers); a row without a start is
left out and counted.

**Moving around.** Drag or scroll to move (Shift scrolls sideways), Ctrl and
scroll to zoom, double-click or **Fit** to see everything. **Time + / -**
zooms only the time axis, **Lanes + / -** only the lanes, so a year of data
fits on one screen with every lane still readable. The view stops at the
first and last bar instead of scrolling on into empty years, and the time
axis labels follow the zoom: years, months, days, then hours. Click a bar to select its row, then switch to Table or Record view
to land on it. The timeline follows the search and column filters.

**Elsewhere.** `octa --overlaps FILE --overlaps-lane room` prints the same
pairs and exits 1 when there are any; the `find_overlaps` MCP tool does the
same for the Assistant.
"#;

pub const FORECAST: &str = r#"# Trend and Forecast

On a **Line** chart over dates or numbers, the row above the plot adds:

- **Trend**: a straight line through the points, or a moving average (one
  season when the dates show one, else 5 points).
- **Forecast**: how many periods to forecast ahead; 0 is off. Each line gets
  a continuation with an 80% range (darker) and a 95% range (lighter), both
  widening further out.

The model is Holt-Winters: level, trend and a season read from the spacing
(24 hourly, 7 daily, 52 weekly, 12 monthly, 4 quarterly), used once there are
two full cycles of history. **Advanced** lets you set the season length
yourself.

The points must be evenly spaced and at least 4. Bucket uneven data first
with **Analyse > Time series**.

**Forecast to table** opens the forecast values and ranges in a new tab.
Exports include the lines; the bands are drawn there as two 95% lines.
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

Above **Settings > Performance > Chart max points** (default 25,000),
Histogram / Line / Scatter evenly-spaced downsample. Bar and Box
always work off the full input.

## Interacting

- **Drag** pans.
- **Mouse wheel** zooms.
- **Right-drag a box** zooms into that region.
- **Double-click** resets to auto-bounds.
- **Hover** a point or bar to see its coordinates in a tooltip.

## On a live database

A chart opened from a database tab that holds only part of its table draws
over the whole table, asking the database; see Analyses on Live Databases.
"#;

pub const SQL_VIEW: &str = r#"# SQL View

The **SQL Query** view exposes the active table to an in-memory DuckDB
connection as a table named `data`. Press **Ctrl+Enter** to run the
query under the cursor. On a large-file tab, `data` is the whole file instead,
scanned where it lies, not the page on screen (see **Large Files**).

- **Mark part of the editor** and Ctrl+Enter (or Run) runs only the marked
  part, so one editor can hold several statements.
- Everything after `--` on a line is a comment and is drawn faded.
- **F12** comments or uncomments the marked lines (or the caret's line):
  `--` goes in front of each line's first character, or, when every
  marked line already starts with `--`, that first `--` is removed.
  Mixed lines all get commented. A `--` after code only goes when the
  selection lies inside that comment.
- **Format** lays the query (or the marked part) out one clause per line,
  indented. Comments are kept and the meaning does not change. The style is
  set under **Settings > SQL**, with a live preview: keywords upper, lower or
  as written; 2 spaces, 4 spaces or a tab; commas at the end of a line or at
  the start of the next; `JOIN` level with `FROM` or under it; how long a
  list, a clause or a bracket may be and stay on one line; blank lines
  between statements; a closing semicolon. **Whole query on one line**
  only tidies spacing and keyword case. **Format
  SQL** can be bound to a key under **Settings > Shortcuts**.
- The editor has line numbers, syntax-aware case conversion (UPPER / lower)
  via right-click, and a chip-style autocomplete row showing matching column
  names, table, schema and catalog names and SQL keywords. On a live
  database tab running on the server, the server's catalogs, schemas and
  tables are offered too, and on any tab whose **Run on** points at a
  connection the columns of every server table the query names are
  fetched in the background. The list shows only while you type a word:
  a click, an arrow key or marking text closes it, so a marked name never
  opens it. **Up / Down** move through the suggestions,
  **Enter** or **Tab** takes one, and **Esc** closes the list but keeps you
  in the editor, so the next **Enter** is a line break. Disable
  autocomplete in **Settings > SQL > Autocomplete** (on by default).
- The faded template in an empty editor becomes real text on **Tab**,
  ready for Ctrl+Enter.
- **Find and replace**: with the keyboard in the editor, **Ctrl+F** opens
  the find box over the editor's top right and **Ctrl+H** the same box with
  its replace row under the find field, as in VS Code. Text marked on one
  line becomes the search. Matches are found in any case and highlighted, the
  box counts them, **Enter** goes to the next one and **Shift+Enter** to the
  previous one. **Aa**, **W** and **.\*** switch to matching capitalisation,
  whole words and regular expressions. **Replace**, or Enter in the replace
  field, swaps the current match, **All** every match; Ctrl+Z in the editor
  takes it back. **Escape** closes the box and returns to the editor.
- **Several carets**: Alt+click or Shift+Alt+Down adds one (by default,
  both can be changed), and typing then happens on every line at once (see
  **View Modes**, Raw text). Autocomplete waits while there is more than one
  caret.
- **Run on** picks where the query runs: **local DuckDB**, or any saved
  database connection. A connection runs the query straight on that
  server, where every table is queryable by its real name without
  attaching anything. A database tab starts on its own connection.
- **Queries run in the background**, local ones included: the result area
  shows a spinner and the time so far, Run turns into **Cancel**, and the
  window keeps responding. `data` (the active table, edits included) is
  copied into DuckDB in the background when the panel opens and again at
  the first Run after the table changed; the status bar says when a copy
  is running. While a copy runs, or a statement that changes `data`
  (UPDATE, DELETE, INSERT), the tab is read-only; a plain SELECT leaves
  it alone.
- **Copying results**: click a cell, **Ctrl+click** more cells,
  **Shift+click** a rectangle, click a **column header** or a **row
  number** for the whole column or row. **Ctrl+C** copies the selection
  as tab-separated text; right-click offers Copy cell, row, column,
  selection and all.
- **Open result as tab...** opens the whole result as a new tab of its
  own, to edit, filter or save like any other table.
- Results render under the editor with a **row counter** above the grid,
  followed by how long the query took in brackets (`1234 result rows
  (84 ms)`, seconds past one second; a failed query is timed too)
  (display-only; it is never part of the data or an export); errors render
  in red.
- Results arrive **one page at a time**. A SELECT is computed once inside
  DuckDB and the first **Settings > SQL > Result rows per page** rows
  (1,000 by default) go to the grid; scrolling near the end fetches the
  next page. The counter shows `1,000 / 8,432,109 result rows`, and the
  total is an exact count over the whole result. Set the page size to 0
  to load everything at once. Export and Write result to DB always cover
  the whole result, never just the page on screen. A result from a query run
  on a connection goes into a DuckDB or SQLite file as the rows the server
  returned: its tables exist only there, so the query is not run again.
- Queries run **on a live database connection** are not paged: they
  honour the initial-load row cap (**Settings > Performance**, default
  2,000,000) and the counter notes "row cap reached" when a result stops
  there.
- **Your other open tabs are queryable** under a SQL-safe version of
  their own tab names, so joining two open files needs no attach step.
  The panel names them once; switch it off under **Settings > SQL >
  Query other open tabs**. The copy runs in the background and the
  status bar says so. Tabs above **Max rows to register** are left out,
  because registering copies their rows.
  A tab from a live database connection is never copied: on the server
  its sibling tables already join by their real names.
- **Ctrl+Shift+E** (default) exports the current SQL result.
- The panel can be docked Bottom (default), Top, Left, or Right via
  **Settings > SQL > Panel position**.
- **Maximise** in the panel header lets the panel fill the window,
  **Restore** docks it again. Opened on an empty tab the panel starts
  maximised; opening a file into that tab docks it.
- **Several editors**: **+** in the toolbar (or **Ctrl+T**) opens another editor beside
  the others, each with its own query and result. Click into one (or its
  result) to make it the one Run, Format, History and Export act on; it
  gets an outline. The small x above an editor closes it.
- **While a query runs** the result area shows a spinner and the time so
  far instead of the previous result, with **Cancel** to stop it. Local
  queries run in the background too, so the rest of the window stays
  usable; a query that changes `data` keeps the tab read-only until it
  is done.
- **The editor stays open** when you close its tab or open a file into
  it: the next tab opens it and takes over your editors, with their files,
  when its own editors are empty. Switch it off under **Settings > SQL > Keep the SQL
  editor open**.

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

The workspace tree, the Inspector, the editor and the results are
stacked panes sharing the panel's height. Drag the line between two of
them to move that boundary: the space comes from the pane beside it, the
others keep theirs, and no pane is ever drawn over another. The panel's
own outer edge gives all of them more room at once.

Each tab owns a persistent SQL **workspace** that outlives individual
runs. The collapsible Workspace section above the editor lists what is
queryable:

- `data` - the active table, unsaved edits included. It is copied in
  again at the first Run after the table changes, so querying an
  unchanged table again costs no copy; **refresh** next to `data` copies
  it at once.
- **+ Add table...** loads more files as extra tables (any readable
  format), for cross-file JOINs. They are read in the background; the
  status bar counts them and its Cancel stops between two files, taking
  back what was added. Cloud objects from Attach cloud go the same way.
- **Attach database...** ATTACHes a DuckDB or SQLite file
  (`alias.schema.table`), in the background. Without the SQLite add-on
  every table is copied instead (`alias__table`), and Cancel stops that.
- **Attach connection** ATTACHes a saved live-database connection
  (PostgreSQL / MySQL read-only via DuckDB extensions; a saved DuckDB or
  SQLite file connection attaches read-only under the connection's name;
  SQL Server,
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
cannot change. An import has no limit on the number of tables: it runs
in the background, the status bar counts the tables done, and its
**Cancel** stops it. It is all or nothing: a cancelled or failed import
takes back what it already brought in, so no table is ever silently left
out. Each table holds at most the maximum rows loaded on open; for more,
query the server directly with Run on server.

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
  each with where it ran (connection, cloud object or file), how long it
  took and how many rows came back. Hover one to see the whole query. Pick
  one to add it below what the editor already holds; it comes in marked,
  so **Run** runs just that query. **Clear history** forgets the lot. It
  is one list for every SQL editor, kept between sessions.
  **Settings > SQL > Query history** switches it off or changes how
  many are kept (20 in total by default, 0 keeps all). Turning it off also deletes what was kept, since
  a query can carry values out of your data. **A picked query replaces
  the editor text** (off by default) makes a query picked here or inserted
  from Snippets replace the editor text instead.
- **Snippets** opens a manager window for a saved library of named queries
  that persists across sessions. Use **Save current query as snippet...**
  to store the editor content under a **name** and an optional
  **description**; each snippet has **Insert** (add it below the editor
  text, marked so **Run** runs just that query, the same as History) and
  **x** (delete). The window has minimise / maximise / close controls
  and is resizable. Snippets live in `sql_snippets.json` in Octa's config
  directory.

## Saving and opening SQL files

The **File** button in the SQL toolbar reads and writes `.sql` files.
**Open...** puts a file in an empty editor, or in a new editor after the
others (appended at the right) when the active one holds text. A file
bigger than **Settings > Performance > Raw view size cap** is refused. **Save** writes the editor back to its file (an editor
without one asks where, like **Save as...**); **Ctrl+S** does the same
while you type in the editor and saves the table everywhere else. The
editor shows the file name above the text, with `*` while there are
unsaved changes. Octa does not ask about unsaved changes on close, so save
first. With Keep earlier versions on, Save keeps the replaced file like any
other save. Files keep their line endings (CRLF stays CRLF). **Open SQL file...**
and **Save SQL file as...** can get a key under **Settings > Shortcuts**.

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

## Explain a query

**Explain** sits beside Run. It shows how the database would carry out a
query, without running it. It takes the marked text, or the whole editor
when nothing is marked, and explains one statement at a time. Nothing is
executed (Octa never uses EXPLAIN ANALYZE), so it is safe on read-only
connections, and explaining a DELETE deletes nothing. A semicolon anywhere
but at the end is refused, even inside text or a comment, so nothing after
it can reach the database.

The plan comes in three views, and the choice is remembered per editor:

- **In plain words** (first): the steps numbered in the order the database
  works through them, with plain names, the table each step reads and the
  rows the database expects. Each step also says what sets it apart, in the
  query's own terms: the condition it keeps rows by, how many rows a limit
  keeps, what a group or sort goes by. A join or union says which earlier
  steps it combines. Steps that have no plain name (internal bookkeeping
  such as Spark's exchanges) are left out here; the other two views show
  every step.
- **Table**: the same steps as rows, with depth, step, table, estimated
  rows and detail.
- **Server's own**: the plan exactly as the server printed it, with every
  technical detail.

Explain works on **local DuckDB**, DuckDB file connections, PostgreSQL, MySQL / MariaDB,
Databricks and BigQuery. On other engines the button is greyed out and its
hover text names the engine. On BigQuery the plan shows the bytes the query
would read, never a price. Databricks plans carry no row estimates, so that
column stays empty there. Databricks also filters while it reads, so the
condition shows on the table read, and it groups and sorts in two rounds
(each part of the data first, then all of it together), so those steps can
appear twice.

On the local DuckDB, Explain does not run while a query runs or the table
is still being copied into SQL; the editor says so. Press it again once
that is done. When the table changed since the last Run, Explain copies it
first, in the background, and shows the plan when the copy is done.

The shortcut **Explain query plan** ships unbound; bind it under
**Settings > Shortcuts**. On the command line use `--explain`, and in MCP
pass `explain: true`.
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

pub const PROTECTED_FILES: &str = r#"# Password-Protected Files

A protected file is not a corrupt file, it is a locked one. Octa opens and
writes **encrypted zips**. It also recognises password-protected workbooks but
cannot open one yet: see the end of this page.

## Saving with a password

**File > Save with password** writes the table into a new password-protected
zip. Pick the file name as in Save as (`report.csv`), then type the
passphrase twice. Only a zip is written, and the window says so every time:
the file on disk is `report.csv.zip`, holding `report.csv`, encrypted with
AES-256. Octa opens it (File > Open); so do 7-Zip (Windows) and Keka (Mac).
The zip tools built into Windows and Mac cannot, so a double-click will not
work. Tick **Remember** to keep the passphrase in this computer's keychain;
otherwise it is stored nowhere.

## Detection comes first

Whether a file is protected is readable WITHOUT the passphrase: a flag in
a zip's central directory, the first eight bytes of a workbook. So a file
that needs nothing is never interrupted by a prompt.

## The prompt

Opening a protected file raises a small window naming the file, with a
passphrase field, a **Remember this passphrase** box, and Open and
Cancel. Enter opens it.

A rejected passphrase clears the field, says so in text you can select
and copy, and asks again. The message never repeats what you tried.

## Remembering

Ticking the box stores the passphrase in your operating system's keyring,
keyed by the file's full path, so two files with the same name in
different folders are different secrets. It is never written to
settings.toml. A stored passphrase that stops working falls through to
the prompt rather than failing with a message about a secret you had
forgotten existed.

## Editing and saving

Edit the table as usual and press **Ctrl+S**: Octa writes your changes
back into the same encrypted zip, with the same passphrase. Any other
entries in the zip are copied across untouched, still encrypted.

The plain copy Octa works on lives in a private temporary folder only
your user account can open. It is removed when you close the tab, when
Octa exits, and, after a crash, the next time Octa starts.

**Save As** to another file writes a plain, unprotected file, and the
tab moves to it: from then on Ctrl+S writes that file, not the zip.

If writing back into the zip fails, the status bar names the zip and the
reason, and the tab stays unsaved, so closing it asks first, and Save
there leaves the tab open while the zip still cannot be written.

Earlier versions and backups are kept of the zip file itself, never of
the plain copy.

## Where a passphrase is never written

- Not in settings.toml, only the OS keyring, and only if you asked.
- Not in the tabs Octa reopens at the next start: a protected tab is
  remembered as its zip and asks for the passphrase again.
- Not in any error. The decryption paths report THAT a passphrase was
  rejected, never what was tried.
- Not in a debug report: the redactor masks anything after a password or
  passphrase label, as a second lock on the same door.

## Protected workbooks

Opening one says it is password-protected rather than reporting a
corrupt file, but Octa cannot decrypt it.

That is a dependency problem, not a feature one. The one maintained
pure-Rust crate for ECMA-376 agile encryption pins a version of its XML
parser that carries two advisories, and the vulnerable code runs on XML
read out of the workbook being opened, which is a file someone else sent
you. Shipping a known denial of service to read a locked spreadsheet is
a bad trade, so the dependency was dropped. It returns when that pin
moves.

To read one now, open it in Excel or LibreOffice and save an
unprotected copy.

## Limits

Encrypted zips only. Save writes back into the zip you opened; a copy
saved with Save As is plain, so a passphrase never silently travels with
it. The first readable table in the archive is opened; an archive of
several tables is not yet offered as a listing. Archives (.zip, .tar,
.tgz, .tar.gz) and compressed files (.gz, .zst) inside an encrypted zip
are not opened, since opening one would write its contents outside the
private folder.
"#;

pub const TAB_MEMORY: &str = r#"# Tab Memory

Octa keeps every open tab's rows in memory, and nothing on screen says
which of eleven tabs is the one holding two gigabytes.

**Data > Tab memory...** lists every open tab with its row count and an
estimated size, plus a total. The shortcut ships unbound; bind one under
**Settings > Shortcuts**.

## The numbers are estimates

An exact figure is not observable from inside the process: a string may
hold more capacity than its length, the allocator pads, and a hash map
keeps spare buckets it does not publish. A precise-looking total would
be a lie with decimals on it.

They are good for comparison, which is the question being asked. The
estimate counts the cells and their text, the pending-edit overlay, the
marks, the column metadata and **both undo stacks**. The undo stacks
matter: a column type conversion snapshots the whole column twice, so a
tab that feels idle can hold two extra copies of a column.

## Unloading

**Unload** drops that tab's rows. The tab stays, keeping its name, its
file, its view mode and its view state, and the file is read again the
moment you select the tab.

Unloading is refused, not warned about, when it would lose data:

- The tab has **unsaved changes** - edits live only in memory.
- The tab has **no file behind it** - a new table, a SQL result, a chart
  tab or a paste has nothing to read back.

The button is disabled in both cases and explains which one applies.
"#;

pub const LOAD_WHOLE_TABLE: &str = r#"# Load Whole Table

A tab does not always hold every row of its source:

- A **database table** opens one page at a time. How big a page is comes from
  **Settings > Performance > Live database page size**, and scrolling to the
  bottom fetches the next one.
- A **file** stops at **Initial-load row cap** in the same section.

Whatever works on the tab's rows, such as Summary, Value frequency, a chart or
the relationship map, then sees only those rows. **Data > Load whole table...**
fetches the rest.

## How it works

1. Octa counts first. For a database it asks the server with one
   `SELECT COUNT(*)`, which reads no rows. A file that knows its own size
   (Parquet, for example) says so straight away. A CSV cannot know without
   reading all of it, and the dialog says that instead of guessing.
2. The dialog shows how many rows the source has and how many the tab holds
   now. Nothing is downloaded until you click **Load all**.
3. The rest arrives in large batches. The status bar shows the progress and a
   **Cancel** button. Cancelling keeps every row that already arrived, and the
   tab can still scroll for more later.

While a whole table loads, Octa does not drop rows from the start of the tab
to save memory, as it otherwise does past three million rows. You asked for
all of them.

## Where to find it

- **Data > Load whole table...**
- Right-click a tab, then **Load whole table...**
- A keyboard shortcut you can set under **Settings > Shortcuts** (none by
  default).

The entry is greyed out when the tab already holds every row.

## Good to know

- A large table takes time and memory. The count in the dialog is there so
  you know what you are asking for.
- A database tab stays a database tab. Edits and **Save** still write back to
  the server exactly as before.
- Another database read already running (opening a table, the next page)
  has to finish first. The dialog says so.
"#;

pub const LARGE_FILES: &str = r#"# Large Files

Large-file mode opens a file read-only, keeps its rows on disk and fetches only
what is on screen. That is the whole trade: every row of a file far bigger than
memory is reachable, and in exchange the tab cannot be edited. Everything that
changes data is off, and everything that works on "the table" works on the one
page of rows on screen.

## Ordinary open or large-file mode

An ordinary open reads the first rows into memory (up to **Settings >
Performance > Initial-load row cap**, 2,000,000 by default) and streams more in
as you scroll down. Large-file mode keeps the rows on disk and holds one page of
2,000 rows at a time.

| Feature | Ordinary open | Large-file mode |
|---------|---------------|-----------------|
| Rows in memory | Up to the row cap, more as you scroll | One page of 2,000 rows |
| Scrolling | Through the loaded rows | Anywhere in the file |
| Editing, inserting and deleting rows and columns, undo | Yes | No, read-only |
| Colour marks, conditional formatting, validation, find and replace | Yes | No |
| Change type, Detect dates, Use as header, column tools | Yes | No |
| Search box | Filters or highlights the loaded rows | Filters the whole file once typing pauses; no highlighting |
| Column Filter, Filter by value | Yes | Filter the whole file; their value lists count the whole file |
| Filter to marked | Yes | No marks in this mode |
| Ask (plain-language filter) | Yes | Filters the whole file |
| Sort by a column header | The loaded rows | The whole file, letter by letter |
| Sort by columns... (several keys) | Yes | Reorders only the rows on screen |
| SQL panel | `data` is the loaded rows | `data` is the whole file, read in place |
| Summary, Chart, Value frequency and other analyses | The loaded rows | The page on screen; the result says so |
| Find duplicates | Every mode | Highlight and New tab, on the page |
| Split into files, Write table to database | Yes | Refused, they would write one page |
| Save As | The loaded rows, asks first when partly loaded | The whole file, or what its filters keep |
| Header row suggestion | Yes | No |
| Assistant | Sees the loaded table | Told the real row count; reads the file |
| Memory | Grows with the rows loaded | About one page |

**To work with every row** of a large file, use the SQL panel: on a large-file
tab its `data` is the whole file, scanned where it lies rather than copied, so
`SELECT state, count(*) FROM data GROUP BY state` counts every row, and the
result can be written out as a new file. A query over the whole file takes
about as long as reading the file once (a second or two for a 5 GB CSV already
in the disk cache), and it runs in the background. A query that returns millions
of rows is collected in full before its first page shows, so give it a `LIMIT`
or an aggregate. On the command line, `octa --sql FILE --query '...' --stream`
does the same.

## Big files opened the ordinary way

A file below the large-file threshold opens the ordinary way, however many rows
it has. For any file over 8 MB, the reading and the preparation that follows
(whitespace trim, header cleaning, numbers, and dates when switched on) run in
the background, so the window keeps responding while the **Loading file...**
spinner turns. One setting changes that, and it is off by default; ticking it
opens a warning saying what follows.

**Settings > File-Specific > Detect dates on load.** Every text column is
checked for dates before the file shows. A column is ruled out on its first
thousand values, but every value of a column that looks like dates is read and
converted: about three seconds per date column at five million rows, added to
the opening time. The window keeps responding. Leave it off for large files and
run **Columns > Detect dates** on the table that needs it; that runs in the
background too, and one Ctrl+Z undoes it.

**Settings > Table View > Cell line breaks** cost little on a big table: a row's
height is measured when it first comes into view, and until then it counts as
one line. On rows of several lines the scrollbar therefore settles as you
scroll.

**Also slow on millions of rows.** These run in the background, so the window
keeps responding, but they take a while and keep the tab read-only meanwhile:

- **Opening the SQL panel** copies the loaded rows into the panel's database,
  about a second per million rows; a Run copies them again only when the table
  changed since. In large-file mode `data` reads the file in place and nothing
  is copied.
- **Answering the date or number question** converts the column, about a
  second per five million rows.
- **Save and Save As** of a big table (Help: **Saving**).

The memory figure in the tab name's tooltip is worked out from a sample of at
most 10,000 rows, so hovering a tab costs nothing at any size.

**Save As on a large-file tab** writes the whole file, not the page on screen,
in the order the view is sorted by; with a search or column filter active it
can write just the rows that filter keeps. DuckDB reads the file and writes the
new one in the background, and the new file appears only once it is complete.
The target can be CSV, TSV, Parquet, JSON or JSON Lines; Save As cannot write
over the file the tab is reading, and Save does nothing, as nothing in the tab
can be changed.

## If it freezes or Octa closes

- **The table shows, then the window stops responding and Octa disappears**
  without a crash report: the loaded rows did not fit in memory. On Linux,
  `journalctl -k` shows `Out of memory: Killed process ... (octa)`. Five
  million rows of an 18-column CSV take about 5 GB, so lower the row cap in
  **Settings > Performance**, or open the file in large-file mode.
- **Opening takes much longer than reading the file should**: Detect dates on
  load is on, or the row cap was raised. A file far bigger than memory is better
  opened in large-file mode.
- **Date columns arrive as text**: that is the default. Use **Columns > Detect
  dates**, or **Change type** for one column.
- **A feature does nothing in large-file mode**: see the table above.

## How the mode works

**When it happens.** Octa decides for you, when you open the file the ordinary
way. There is no separate command for it. The mode kicks in for files of at
least 2 GB. A Parquet file is measured by the larger of its size on disk and
the size of its data once decompressed, which its footer states without being
read, so a small file that expands to gigabytes is caught too; a CSV states
nothing, so its size on disk counts. The row count never decides on its own: an
ordinary open loads the first rows and streams the rest in as you scroll. The
threshold lives under **Settings > Performance**. A dialog explains the
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
would lock the window. Column Filter and Filter by value work the same way: the
ticked values join that condition, so they keep every matching row of the file,
exactly the rows an ordinary tab would keep, and their value lists are counted
over the whole file in the background. The comparisons the Ask box makes
(`amount > 1000`) join the same condition and keep exactly the rows they would
keep in an ordinary tab. A column of lists, structs or binary data cannot be
matched in a query; its filter is left out and the status bar says so.

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
CSV a longer value shifts every byte after it. To change a large file, query it
in the SQL panel on the tab, where `data` is the whole file, and write the result
out as a new file.

**The assistant sees the file, not the page.** A large-file tab holds one
2,000-row page, so the assistant is told the file's real row count plus which
rows are on screen, and a tool asked for "the open tab" reads the file from
disk instead of the page. Without that, **Explain this file** would describe
2,000 rows of a billion-row file and never say it was looking at a slice.

**Nothing holds up the window.** Opening the file (which counts its rows), every
page while you scroll or jump, and every new sort or filter are read on a
background thread: **Reading the file...** shows in the status bar and the rows
on screen stay until the new ones arrive. While one page is being read the
newest request waits for it, so dragging the scrollbar reads one page at a time.
A deep position in a very large CSV still takes the time to walk the file;
Parquet skips straight to it.

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
  path with the same codec. Save as to a plain extension writes
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

pub const BINARY_CELLS: &str = r#"# Binary Cells

Some cells hold raw bytes instead of text or numbers: a picture stored in a
database, a PDF, a zip. The table shows them as a short placeholder. The
viewer opens them up so you can see what is inside.

## Opening it

Right-click a binary cell and choose **View contents...**. There is also a
**View cell contents...** shortcut for the selected cell. It ships unbound;
give it a key under **Settings -> Shortcuts**.

Clicking a picture in the Notebook view opens it here too.

## What you see

Octa reads the first bytes of the cell, never a file or column name, to tell
what it holds.

- PNG, JPEG, GIF, WebP, SVG: the picture and its size in pixels. A very
  large picture is shrunk on screen only. An SVG never loads a file or web
  address it points to.
- PDF: the page count and the text of all pages.
- Zip: the files inside, with their sizes and dates. Nothing is unpacked.
- Gzip: what the data unpacks to, as text or as a hex dump. Unpacking runs
  in the background and stops only at the limit under **Settings ->
  Performance** (or never, with Unlimited ticked there).
- JSON: the text, laid out with indentation.
- Anything else: a hex dump of the first 64 KB.

Text (from a PDF, a JSON cell or unpacked gzip) shows its first 256 KB.
Save to file gets all of it.

A cell that looks like a picture but is damaged shows the reason and falls
back to the hex dump.

## Getting the bytes out

- **Save to file...** writes the cell's bytes to a file you choose. The
  suggested name carries the right extension.
- **Open in default app** writes a copy to a temporary folder and hands it to
  the program your system uses for that kind of file. The copy is deleted
  when Octa closes. Octa runs nothing itself.
"#;
