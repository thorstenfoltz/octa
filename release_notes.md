A big release. Octa now reads REST APIs and the tables inside PDFs, replays
your clean-up steps on next month's file, merges copies of a table that
several people edited, and draws rows with a start and an end on a timeline
that shows where they overlap. Around that: a proper dialog for changing a
column's type, checks for IBANs, card numbers and VAT numbers, a test data
generator, three new join types, git marks in the folder sidebar, and five new
themes. It also opens log files as tables, forecasts a chart line ahead, joins
by location, follows a cell back through its Git history, and finds the lookup
tables hiding inside flat exports and the stray formats hiding inside columns.
Octa also got more honest about partly loaded data: a search, a SQL result or a
save that only sees part of the table now says so.

## What's new

### Read a REST API as a table

**Settings -> API endpoints** saves an endpoint once: base URL, path,
authentication (bearer token, API key, username and password) and pagination
(page number, offset, cursor or `Link` header). Opening it walks every page and
gives you one table. **Test** fetches the first page and lists the arrays it
found, so you can pick where the rows are instead of typing it. The credential
stays in your OS keyring. The same endpoints work from the command line
(`octa --api NAME`) and for the assistant (`list_api_connections`,
`query_api`), which can only reach the hosts you saved.

### Tables from PDFs

Open an invoice, a bank statement or a report and Octa finds the tables inside.
One table opens straight away; with several, a picker lists them by page. A
page that is only a scanned image cannot be read without OCR, and Octa says so
instead of opening an empty table.

### Recipes: do the monthly clean-up once

Octa records what you do to a table: renames, type changes, removed
duplicates, filled gaps, transforms, even values you typed by hand (found again
by an ID column, so they land on the right row next time). **Save recipe...**
writes it as an `.ocp` file (Octa reCiPe, plain text) and **Apply recipe...**
replays it on next month's file. Steps refer to columns by name, so a file with
its columns in a new order still works, and a step that no longer fits stops
the replay rather than half-applying it. Recipes also run from the command line
(`--recipe`) and through the assistant (`apply_recipe`). An optional Settings
switch saves a recipe automatically.

### Merge versions

**File -> Merge versions...** puts copies of one table back together after
several people edited them: two versions or ten. Rows are matched by key
columns (Octa suggests them) and changes are merged cell by cell. It asks only
where two people changed the same cell differently. With the original the
copies came from, everything that only one person changed merges on its own;
without it, every difference asks. Also on the command line (`--merge`), for
the assistant (`merge_tables`), and as a git merge driver for data files.

### Timeline and overlaps

**View -> Timeline** draws every row with a start and an end as a bar,
grouped into lanes (a room, a person). Bars that overlap inside a lane are
outlined: a room booked twice, someone on two shifts at once. **Open
overlaps...** opens a tab with one row per overlapping pair, its columns named
after yours (`room`, `row_a`, `check_in_a`, ...) and explained when you hover
the header. Click a bar to select its row. `octa --overlaps` prints the same
pairs and exits 1 when there are any, so a nightly job can catch a double
booking; the assistant has `find_overlaps`.

### Change column type, without losing values

**Columns -> Change type...** shows what a conversion would do before it does
it: how many values convert, which ones do not, and which date format it
detected. Values that do not fit are kept as they were and flagged, never
blanked, and F10 / Shift+F10 step through them. A strict mode refuses
instead. The header's quick **Change type** menu still converts in one click
when everything fits, and now opens the preview instead of greying out when
one value does not.

### Check IDs: IBAN, card numbers, barcodes, VAT, email

**Data validation** has new kinds that check the check digits: IBAN (length and
mod 97 per country), payment card numbers, EAN, UPC and ISBN barcodes, EU VAT
numbers and email addresses. Everything runs locally. A failed check only turns
the cell red, it never blocks anything. **Tidy ID format** removes the spaces
and dashes people type into IDs, and recipes record it.

### Generate test data

**Data -> Generate test data...** makes new rows shaped like the real ones:
same columns, similar distributions and null rates, IDs that stay unique, and
links between tables that still join. Nothing real is copied, so you can send
it to a supplier or attach it to a bug report. The same seed gives the same
rows. Also `octa --test-data` and the assistant's `generate_test_data`.

### Semi, anti and as-of joins

**Join tables** gained three types. **Semi** keeps left rows that have a
partner ("which customers have ordered?"), **Anti** the ones that do not
("which never have?"), and **As-of** gives every left row the nearest right
row in time, such as the price valid at the moment of a trade. Hover a join
type in the list for what it keeps. Also on the command line and for the
assistant.

### Log files as tables

Open an nginx or Apache access log, a syslog, JSON or logfmt lines, or a Java
or Python application log, and Octa reads it as a table: a sortable
timestamp, a level that means the same thing everywhere (`warning`, `warn` and
`W` all become `WARN`), and the format's own fields as columns. A stack trace
stays with the error it belongs to, and a line that fits nothing keeps its own
row instead of disappearing. A `.log` that is not a log still opens as text.

### Trend and forecast

A Line chart over dates can draw a trend line and forecast each line ahead,
with an 80% and a 95% range that widen the further out you look. The season
is read from the dates (7 for daily data, 12 for monthly, and so on), so there
is nothing to tune. **Forecast to table** opens the numbers in a tab, and the
command line (`--forecast`) and the assistant (`forecast`) use the same model.

### Spatial join

The Join dialog has a new **Spatial** type: give every customer the sales
region they are in, or the nearest store and how far away it is, measured over
the earth's surface. Several layers can be joined at once. A layer in a
coordinate system other than latitude/longitude is refused with a message
rather than matching nothing. Picking **Spatial** puts the tab with the points
on the left for you, even when the regions file was the last one you opened.
Also `--spatial-join` and `spatial_join`.

### Cell history

For a file kept in Git, right-click a cell and choose **Cell history...** to
see every commit that changed it, with who and when. The row is followed by a
key, so a re-sorted file does not look as if every row changed, and renames are
followed. Drag the line under the column list to give it more room. When the
entry is greyed out, hovering it shows why, including git's own message. Also
`--cell-history` and `cell_history`.

### Find lookup tables

**Analyse -> Find lookup tables...** finds columns that always follow another
one, like a customer's name and city next to the customer ID on every order,
shows the rows that break the pattern (usually typos), and can split the
lookup back out into its own table without touching yours. Also `--lookups`
and the assistant's `find_lookups`.

### Value shapes

The column funnel has a **Shapes** switch that shows what the values look like
with the specifics taken out: `D-80331` is `A-99999`. A postcode column with
three values typed in another format now shows them at a glance, and ticking a
shape filters to it. The Data quality report flags columns with mixed shapes.
Also `--shapes` and the assistant's `value_shapes`.

### Finding your way around a table

- **Column navigator**: a docked panel listing every column with a search box,
  to show, hide, freeze and reorder columns without scrolling for them.
- **Filter by value**: every header has a funnel that lists the column's most
  common values with counts; tick the ones to keep. Right-clicking the header
  and choosing **Filter values...** opens the same popup. Active filters show
  as chips above the table, each with an `x`.
- **Filter by value or shape** moved from the Search menu to **Columns**. It is
  the same filter as the funnel, in a window with a column picker and a
  **Find** field, and it has the **Values / Shapes** switch too.
- **Edit audit trail**: a docked panel listing every unsaved cell edit, before
  and after, so you can check what a save will change.
- **Tab memory** (Data menu): how much memory each open tab holds, with an
  **Unload** button that frees a tab's rows until you need them again.
- **Refresh a tab** (Ctrl+R) reads its file, database table, cloud object or
  API endpoint again, in the same tab. A Settings choice decides whether it
  asks, refreshes in place or opens a new tab.
- **Search in the Databases and Cloud sidebars**: a search box under each
  header narrows the tree, and **Search all** looks through every expanded
  connection for tables, objects and folders.
- **Git marks in the folder sidebar**: files with uncommitted changes and files
  changed on the current branch are coloured and marked (`M`, `A`, `D`, `U`,
  `*`), with the state spelled out on hover.

### Files and folders

- **Encrypted zips open**: Octa asks for the passphrase and can keep it in your
  OS keyring if you tick the box. A password-protected workbook is recognised
  as locked, not reported as damaged.
- **Combine a folder into one table**: Harmonise schemas can now fold a folder
  of files into a single table, with a `source_file` column saying where each
  row came from (`--combine` on the command line).
- **Cloud folders download in parallel**, with the number of simultaneous
  downloads under Settings -> Performance.

### Honest about partly loaded data

- **Search the whole file.** When only part of a big file or a live database
  table is loaded, the search bar says it is searching the loaded rows, and
  offers a search over the whole source with the same options (match mode,
  case, whole word).
- **SQL results are paged**, with the exact row total, instead of stopping at
  the row cap. Export still writes every row.
- **Every open tab is a SQL table**: open tabs are registered in the SQL
  workspace automatically, so you can join them without attaching each one.
- **Saving a partly loaded file warns you** before it overwrites the rest of
  the file with the part that was loaded.
- Result tabs built from partly loaded data (summary, transpose, sample,
  quality report and the like) say so.

### Views

- **Compare view**: the left pane of a text diff is a real editor now, so you
  can fix the working file right next to the committed version.
- **Raw view** can wrap long lines instead of scrolling sideways.
- **JSON inside JSON**: a string holding JSON unfolds into real tree rows in
  the JSON view.
- **Colour marks work on read-only tabs**, since marking is not an edit.
- **Five new themes**: Deep Sea Contrast (dialogs stand out from the panels
  behind them), Solarized Light, Tokyo Night, Phosphor, and Colour-blind Safe.

### Assistant: Plan mode

In **Plan** mode the assistant does not change your table while it works.
Ask for something broad, such as "clean this file up", and it comes back with
a numbered plan. Untick the steps you do not want, see each step's before and
after, then **Apply**, ask for a revision, or discard it. Sorting rows through
the assistant is now undoable too.

## Fixes

- **Ctrl+R opened a second tab** instead of refreshing, and did nothing on
  database tabs. It now reloads in place, and a failed read leaves the tab as
  it was.
- **Ctrl+M did nothing on database tabs** while the menu entry worked.
- **The git compare view could not be edited.** Both panes accepted typing and
  threw it away on the next frame.
- **The SQL panel's panes overlapped** and a dragged pane snapped back to its
  old size. They are one splitter now.
- **Change type greyed out** on a column with a single bad value, and refused
  dates that were not written in ISO format.
- **Transform column** made one undo step per cell; it is one per transform
  now. Renaming a column in its header could not be undone; now it can.
- **Code colouring in light themes** other than Light used the dark palette.
- **Controls out of line**: in several dialogs and bars (join, timeline, SQL,
  chart) a drop-down sat a few pixels lower than the button beside it.
- **Timeline**: the chart could not be scrolled, the hover text of the top
  bars was cut off, and some lane names went missing with many lanes.
- **Chart axis min and max were ignored** once the chart was on screen: typing
  new values, or clearing them back to auto, kept the old range. The chart now
  follows every change.
- **Git features missed files in a repository** when the file was opened
  through a symlinked folder or by a relative path (`octa data.csv` from a
  terminal). Compare with Git revision, the merge-conflict check and Cell
  history now find them.

## Under the hood

- **Rows of controls share one helper** that gives every widget in a row the
  same height, with a test that measures a real row. The mismatch between a
  drop-down and a themed button had caused the "out of line" reports in four
  features.
- **Large modules were split** (tabs, central panel, table view, SQL panel)
  into files with one job each. No behaviour changed.
- **Menu labels use sentence case** in English.
- **The documentation** covers every new feature in both the in-app Help and
  the website, and `samples/features/` holds small files for trying the new
  joins, ID checks, test data, the timeline, lookup tables (`orders_flat.csv`),
  value shapes (`postcodes.csv`), log files (`access.log`, `app.log`), the
  spatial join (`stores.csv`, `regions.geojson`) and the forecast
  (`monthly_sales.csv`).
