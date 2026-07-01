//! Static Markdown bodies for the in-app documentation dialog. One `const &str`
//! per section; the parent module's `sections()` joins them with the live
//! shortcut table at render time. Split out of `documentation/mod.rs` purely
//! to keep the dialog code itself readable - no behavioural change.

pub(super) const GETTING_STARTED: &str = r#"# Getting Started

Open a file from **File > Open** (or **Ctrl+O**), pick one or more from the
**File > Recent Files** submenu, or pass paths on the command line:

```
octa path/to/file.parquet other.csv
```

Multiple files open into separate tabs.

Drag-and-drop from the OS file manager is **not** wired up. On Linux
Wayland sessions winit does not deliver drop events, and Octa does not
subscribe to them on the other platforms either. Use **File > Open**
to open files.

## Read + write formats

- Tabular columnar / data-science: Parquet, Avro, Arrow IPC, ORC
- Plain text / interchange: CSV, TSV, JSON, JSONL, XML, TOML, YAML
- Office: Excel (`.xlsx`), OpenDocument Spreadsheet (`.ods`)
- Databases (diff-on-save row edits, no schema changes): SQLite (`.sqlite`,
  `.sqlite3`, `.db`), DuckDB (`.duckdb`, `.ddb`), GeoPackage (`.gpkg`)
- Statistical: SPSS (`.sav`, `.zsav`), Stata (`.dta`)
- Other: dBase / DBF, Jupyter notebooks (`.ipynb`), Markdown (`.md`),
  Plain Text
- Source / config text with syntax highlighting (`.py`, `.rs`, `.go`, web
  markup, ...). Extension-less container files (`Dockerfile`, `Dockerfile.*`,
  `Containerfile`, `Containerfile.*`) are recognised by name, highlighted, and
  listed in the sidebar file browser.

## Read-only formats

- SAS (`.sas7bdat`)
- R Datasets (`.rds`, `.rdata`, `.rda`)
- HDF5 (`.h5`, `.hdf5`, `.hdf`)
- NetCDF v3 (`.nc`)
- NumPy (`.npy`, `.npz`)
- MessagePack (`.msgpack`, `.mpk`)
- BSON (`.bson`)
- EPUB (`.epub`)
- GeoJSON (`.geojson`)
- Shapefile (`.shp`)
- Delta Lake / Apache Iceberg (table directory; **File -> Open table folder...**)

When saving, the original format and settings (e.g. CSV delimiter) are
preserved. Database writes only update changed rows and reject schema
changes; rename or add columns in another tool first.

## Multi-sheet Excel

Each worksheet of an Excel workbook is treated as a table. Workbooks
with up to N sheets (default 5, **Settings > Performance > Excel sheets
to auto-open**) open all sheets at once, each in its own tab. With more
than N sheets, a picker lets you choose which to open (you can pick more
than N, or all).

## Repairing a malformed CSV / TSV

Turn on **Settings > File-Specific > Offer repair on malformed files** and,
when a CSV/TSV reads but looks malformed (bad encoding, a byte-order mark,
stray control characters, a delimiter that disagrees with the extension, or
rows with uneven column counts), Octa offers to clean it up on open. It lists
what it found, shows a preview, and lets you **Repair and open**, **Open
without repair**, or **Cancel**. When some rows have *more* fields than the
header, a **Keep extra values (add columns)** option widens the table so the
extra fields keep their own columns (named `column_4`, `column_5`, ...) instead
of being dropped; short rows pad with empty cells. The file on disk is never
changed.
"#;

pub(super) const NAVIGATION: &str = r#"# Navigation & Selection

- **Arrow keys** move the selected cell.
- **Scroll wheel** scrolls vertically; **Shift + Scroll wheel** scrolls
  horizontally.
- Click a **row number** to select the entire row (Ctrl+click adds; Shift+click
  picks a range).
- Click a **column header** to select the entire column.
- **Ctrl+A** selects all rows (when no text editor is focused).

Jumps and extends:

- **Ctrl+Shift+Arrow** jumps the selected cell to the first/last row or column.
- **Ctrl+Arrow** extends the row or column block by one in that direction.

Use the navigation field in the bottom status bar (**Ctrl+G**) to jump to a
cell by `R5:C3`, `R5`, `C3`, a row number, or a column name.
"#;

pub(super) const EDITING: &str = r#"# Editing & Undo/Redo

- **Double-click** a cell to start editing; the current text is selected so
  you can type to replace it, or click to position the cursor.
- Click outside the cell or press **Tab** / **Enter** to confirm; **Escape**
  cancels.
- **Undo** (Ctrl+Z) and **Redo** (Ctrl+Y) cover cell edits, row/column
  insert/delete/move, and color marks. Both are also available in the **Edit**
  menu and remappable in **Settings > Shortcuts**.

Structural edits:

- **Edit > Insert Row** adds a new empty row below the selected cell.
- **Columns > Insert Column** opens a dialog to add a column (name + type).
- **Edit > Delete Row** and **Columns > Delete Column** remove the selected one(s).
- **Edit > Move Row Up/Down** and **Columns > Move Column Left/Right** reorder data.
- **Edit > Discard All Edits** reverts all unsaved changes.
- **Drag a column header** to reorder columns.
- **Double-click a column header** to rename it inline.
- **Right-click a column header** to change the column data type.

## Copying

**Ctrl+C** copies the current selection (single cell, row block, column
block, or free multi-cell selection) as tab-separated values. To copy the
same selection as a **GitHub-flavoured Markdown table** with column
headers, use **Edit > Copy as Markdown table** or the **Copy as Markdown
table** entry in the cell / row right-click menu. Pipes and line breaks in
cells are escaped so the table stays well-formed - handy for pasting into a
pull request or Markdown document.

## Number display

Numeric columns show **thousand separators** by default
(`1,234,567.89`). This is display-only; saved/exported data keeps raw
values. Toggle it, or switch English (`1,234.56`) vs European
(`1.234,56`) style, under **Settings > Table View** (**Thousand
separators** + **Number style**).

Right-click a numeric column header (or **Columns > Number format...**) for
a per-column **rounding format**. The dialog applies live (no Apply
step) and is movable/resizable. Type the number of **Decimals** (empty =
Auto; a negative count rounds before the decimal point, e.g. -2 = nearest
100) and pick a rounding mode (Normal / Up / Down). Fixed decimals pad
with trailing zeros. Formats are display-only and per-tab; on **Save**
Octa asks whether to write rounded values or full precision.

## Whitespace trimming on load

By default Octa strips leading/trailing whitespace from string cells
**and column titles** when a file opens (interior spaces are kept), and
shows a banner listing which columns changed. Both the trimming and the
banner can be turned off under **Settings > File-Specific**.

Saving an edited file is described under **Saving**.
"#;

pub(super) const FORMULAS: &str = r#"# Formulas

Cells support simple Excel-like formulas starting with **=**.

- **Cell references**: A1, B2, AA1, etc. (column letter + 1-based row number;
  the column letter appears in each header).
- **Operators**: `+`, `-`, `*`, `/`.
- **Parentheses**: `(A1 + B1) * 2`.
- **Numeric literals**: `=A1 * 1.5`.

When inserting a column via **Columns > Insert Column**, you can type a formula
into the **Formula** field. The formula is treated as a row-1 template and
applied to every row (e.g. `=A1+B1` becomes `=A3+B3` on row 3).

Division by zero leaves the cell empty.
"#;

pub(super) const SEARCH: &str = r#"# Search & Replace

The toolbar search box matches rows in real time. Three modes (selectable in
the dropdown next to the box):

- **Plain**: case-insensitive substring.
- **Wildcard**: `*` matches any sequence, `?` matches one character.
- **Regex**: full regular expression syntax.

## Case, whole word, and scope

Three controls beside the search box refine matching:

- **Aa** toggles **case-sensitive** matching. Off (the default) matches
  regardless of capitalisation in every mode.
- **W** toggles **whole-word** matching, so `cat` matches the word "cat"
  but not "category" or "scatter".
- The **scope** dropdown limits the search to a single column or, by
  default, **All columns**. The dropdown always shows the current scope so
  you can see at a glance whether you are searching one column or the whole
  table.

These apply to the table filter and to the in-place highlight.

## Search history

Recent search queries are remembered across sessions. When there is
history, a **Recent** dropdown appears beside the search box; pick an
entry to re-run it. How many queries are kept is set by **Search history
size** under **Settings -> Search & Editor** (default 5; set it to 0 to
turn the history off). The list is stored in `search_history.json` in
Octa's config directory.

## Filter or highlight

A toggle button beside the search box switches how matches are shown:

- **Filter** (the default): non-matching rows are hidden, as before.
- **Highlight**: every row stays visible and the matching cells are
  highlighted in place.

The default is set in **Settings -> Search result display**. The table view
honours the toggle. Text and tree views (Jupyter notebooks, the JSON and YAML
trees, Markdown and the raw text editor) always highlight, because hiding free
text or collapsing tree nodes makes no sense there.

When matches are highlighted, the search bar shows a **count** (current / total)
and two buttons to step through matches. **Enter** jumps to the next match and
**Shift+Enter** to the previous one while the search box is focused; the view
scrolls the current match into view.

**Ctrl+F** focuses the search box from anywhere; **Ctrl+H** opens the
**Find & Replace** bar above the table:

- **Next** replaces the first match found.
- **All** replaces every match across visible rows.

**Escape** closes the replace bar.
"#;

pub(super) const MULTI_SEARCH: &str = r#"# Multi-search

The toolbar **Search** field filters the active tab. **Multi-search**
covers the other half of the problem: find the same string across
**every open tab** or **every file in a directory** at once.

Open via **Search > Multi-search...** or **F6** (remappable). A docked
panel slides up at the bottom of the window with its own query box,
mode picker, and scope selector.

## Scopes

- **All Open Tabs**: walk every loaded tab. Runs synchronously, no
  background thread -- cheap even with several tabs open.
- **Directory**: walk every readable file in a folder (top level only,
  not recursive). Runs in a background thread; results stream into the
  panel as files finish parsing. Use the **Pick directory...** button
  to choose the folder.

## Modes

Plain / Wildcard / Regex -- same semantics as the main search bar.
Invalid regexes surface a one-line error above the result list.

## Jumping to results

Each result row reads:

    <source>  row N  <column name>  <snippet>

Clicking jumps to that cell. Directory-scope hits that aren't already
open get loaded into a fresh tab first.

## Limits

- **Per-file size cap** (Settings > Performance > Multi-search file
  cap, default 50 MB). Oversized files end up in the skipped chip
  (see below) with their actual size.
- **Cap of 10,000 hits per scan**, 1,000 per file -- a runaway regex
  on a huge dataset can't pin the UI.
- **In-memory rows only**. For lazy formats (Parquet, CSV/TSV) the
  scan covers whatever's currently loaded; rows still streaming in
  the background aren't searched until they land.

## Skipped files

When a reader fails on an individual file (binary blob, malformed
text, encoding mismatch, ...) Octa moves on to the next file and
collects the failing one in a **N file(s) skipped -- click to
expand** chip above the result list. The expanded view shows each
file's name plus the reason (size cap or parser error); the full
path is visible on hover. The list resets on the next search.
A failure in one file does not hide results from files that
searched fine.

Press **Cancel** to stop a running directory scan at the next file
boundary. Whatever hits were already collected stay in the panel.
"#;

pub(super) const COLUMN_FILTER: &str = r#"# Column Filter

Excel-style per-column value-set filter. Pick a column, see its unique
values as checkboxes, uncheck the ones to hide.

## Opening the dialog

- **Search > Column Filter...** in the toolbar.
- The default shortcut (remappable; check Settings > Shortcuts for the
  current binding) opens the same dialog.
- **Right-click any column header > Filter values...** opens the dialog
  pre-seeded on that column.
- The status-bar **Filter** chip (visible when any column has an active
  filter) opens the dialog on the first filtered column.

## Using the dialog

- The top combo picks the column being filtered. Switching columns
  commits the in-progress checks to the previous column automatically,
  so multiple filters can be edited in one session.
- **Find** narrows the value list when a column has many unique values.
  Up to 5000 values are shown at a time; if more match, a hint tells you
  to narrow further with the search box.
- **Select all** and **Select none** operate on the currently visible
  (post-search) subset, not the whole list.
- **Apply** commits the draft. "All checked" and "none checked" are
  both interpreted as "no filter active" for that column.
- **Clear filter on this column** removes the column's filter entirely.
- **Cancel** discards the in-progress draft.

## Behaviour

- Column filters AND with each other: a row must satisfy every active
  column filter to remain visible.
- Column filters also AND with the toolbar text search.
- A small accent-colored dot appears next to filtered column headers so
  active filters are visible at a glance.
- Filters live with the tab. Closing the tab discards them; they are
  not saved to disk.
- "Select none + Apply" hides every row in the current view, just like
  unchecking every checkbox by hand. Use "Clear filter on this column"
  to remove the filter entirely.

## Saving filtered data

**File > Save As** writes only the **currently visible** rows when a
filter (text search or column filter) is active. The on-disk file is a
snapshot of the view; the in-memory table is left untouched so you can
keep working on the full dataset.

Regular **File > Save** always writes the **full table** back to the
source path. The visible filter does not change what Save writes; this
keeps the source file safe from accidental data loss while filters are
active.
"#;

pub(super) const COLUMN_TOOLS: &str = r#"# Column Tools

## Hide and show columns

Right-click any column header and pick **Hide column** to remove it
from the view. Hidden columns are still part of the table on disk:
Save and Save As both write them out. Use **Columns > Show hidden
columns** to bring everything back at once. This is a per-tab,
session-only setting; closing the tab or reopening the file clears
the hidden set.

## Copy column name(s)

Right-click any column header and pick **Copy column name(s)** to
copy the header text to the clipboard. If you have multiple columns
selected (Ctrl-click their headers) and right-click one of them, all
selected names are joined with newlines. Useful for building SQL
SELECT lists or scripts from Octa's view of the file.

## Freeze columns

Right-click a column header and pick **Freeze columns up to here** to
pin that column and every column to its left, exactly like freezing
panes in a spreadsheet. The pinned columns stay visible at the left
edge while the rest of the table scrolls horizontally underneath, so
an ID or name column never scrolls out of sight in a wide table. A
thin separator marks the boundary; **Unfreeze all columns** in the
same menu reverts.

The freeze is per tab and session-only, like column widths. If the
window gets too narrow to keep the whole frozen band and still scroll,
Octa temporarily pins fewer columns and restores the full band when
there is room again.
"#;

pub(super) const VALUE_FREQUENCY: &str = r#"# Value Frequency

Open via the column-header right-click **Value frequency...** entry,
**Analyse -> Value frequency...** (which asks you to pick a column
first), or **Ctrl+Shift+I** (remappable; with no cell selected it opens
the same column picker). The dialog lists the most common values in one
column, ranked by count.

Each row shows:

- The distinct value (or numeric range, when binning is on).
- The count of cells matching it.
- That count as a percentage of non-null cells.

The footer reports total distinct values, total non-null cells, and
the null count. Rows are sorted by count descending; ties broken
alphabetically.

## Top-N

The toolbar offers **Top 20 / 50 / 100 / 500 / All**. The default is
**Top 50**. The choice persists per tab while the dialog stays open.
(Hidden while binning is on, since the bin count is the control there.)

## Numeric binning (histogram)

For numeric columns, a **Bin numeric values** checkbox builds a
histogram: the value range [min, max] is split into N equal-width
ranges (width = (max - min) / N) and each row counts how many values
fall in that range.

Type N into the **Bins:** field (1..1000), or leave it empty for an
automatic count via Sturges' rule (`ceil(1 + log2(n))`, clamped 5..30).

- N bins = N rows: every range is shown in ascending order, including
  empty ones (count 0), so the row count always matches what you asked.
- Labels are `[lo, hi)` half-open (last bin closed `[lo, hi]`).
- An all-identical column has no range to split, so you get one bucket.

NaN, +Inf, and -Inf show up as separate rows after the bins so type
drift is visible. Non-numeric columns hide the checkbox.

## Acting on a row

Right-click a row (when binning is off) for:

- **Copy value** - the raw value to the clipboard.
- **Filter table to this value** - adds a column filter restricting
  the active table to rows where this column equals the picked value.

The bottom **Copy as TSV** button copies the whole visible table as
`<column>\tcount\tpercent` lines.
"#;

pub(super) const FIND_DUPLICATES: &str = r#"# Find Duplicates

Open via **Search > Find duplicates...** or **Ctrl+Shift+D** (remappable).
A modal lists every column with a checkbox - tick the ones you want
to use as the dedupe key. Two rows are duplicates when every checked
column has the same displayed text.

Output modes (radio buttons):

- **Highlight rows in place (Orange mark)**: every duplicate row in
  the active table gets an orange row mark. Use **Edit > Mark > Clear
  all marks** to remove them. Your other marks share the same path.
- **Open duplicates in a new tab**: clones the columns + just the
  duplicate rows into a fresh scratch tab. The source tab is left
  alone; the new tab has no source path so Save prompts.

Notes:

- The Apply button is greyed until at least one column is checked.
- A row whose key only matches itself is not a duplicate - results
  always come in pairs or larger groups.
- Hashing is text-based, so `Int(1)` and `Float(1.0)` render as `"1"`
  vs `"1.0"` and therefore do *not* dedupe. Change the column type
  first if you want them to.
- If no duplicates are found, the status bar reports it and the
  active table is unchanged.

The dialog seeds the key with whatever column is currently selected,
so Ctrl+Shift+D -> Apply is the fastest path for a one-column dedupe
check.
"#;

pub(super) const FUZZY_DUPLICATES: &str = r#"# Find Near-Duplicates

**Search > Find near-duplicates...** (Ctrl+Shift+U) finds rows that are
*almost* the same on the columns you choose, not just exactly equal. It catches
typos, spacing, and reordered words (for example "Jon Smith" vs "John Smith",
or "ACME Inc" vs "ACME, Inc.") and groups the likely duplicates into clusters
with a similarity score for review.

## Controls

The dialog has **two** column choices that do different jobs:

- **Columns to compare**: the columns whose text is matched loosely (where typos
  and near-misses are found). Each candidate row pair is scored per column and
  the scores are averaged; the pair matches when the average is at or above the
  threshold.
- **Only look for duplicates within the same**: an optional column whose value
  must match exactly before two rows are even compared. Think of it as sorting
  the table into bins first, then hunting for duplicates inside each bin only.
  Example: with Columns to compare = name and "within the same" = country, the
  two US rows "Jon Smith" / "John Smith" are compared, but a German "Jon Smith"
  is never compared with the US ones. It makes large tables fast and avoids
  merging rows that clearly differ on a field you trust. Leave it empty to
  compare every row.

Other controls:

- **Method** - how two text values are scored:
  - Edit ratio: counts single-character changes. Best for typos.
  - Jaro-Winkler: rewards matching starts. Best for names and short strings.
  - Token set: compares the set of words, ignoring order and punctuation. Best
    when words are reordered ("Jon Smith" vs "Smith, Jon").
- **Similarity threshold**: how alike two rows must be (default 85%). 100% =
  identical; lower catches looser matches but risks false matches.
- **Normalise**: ignore case, collapse spaces, and ignore punctuation before
  comparing (all on by default). This is what lets "ACME, Inc." line up with
  "ACME Inc".
- **Row limit** caps how many rows are scanned (default 20,000); if the table
  is larger, the result says so.

The scan runs in the background with a **Cancel** button. Clusters are formed
transitively: if A is near B and B is near C, all three land in one cluster.
The cluster's reported score is the lowest linking similarity inside it (the
honest worst case).

## Output (tick any combination)

- **Add a cluster_id column** (default): writes a cluster_id and cluster_score
  column onto the table so you can sort or filter by cluster. One undo step.
- **Highlight** colours the near-duplicate rows orange. Re-running first clears
  the previous run's highlight, so it never builds up into a fully marked table,
  and your own marks are left alone.
- **New tab** opens a clustered report: a cluster id and score column followed
  by the original columns, grouped by cluster.

The same scan is available as the `fuzzy_duplicates` MCP / assistant tool.
"#;

pub(super) const SUMMARY: &str = r#"# Summary

The Summary tab answers "what does this table look like?" in one click.
It is the GUI counterpart of the CLI's `octa --describe` and of pandas'
`df.describe()`: one row of statistics per column of the active table.

## Opening it

**Analyse > Summary...** opens a new tab named `Summary - <file>` for
the active table. Unsaved cell edits are included: the statistics
describe the table as you currently see it, not the file on disk.

## What it shows

One row per source column. The column headers are short, lower-case
identifiers (`column_name`, `not_null`, `total_rows`, ...) so the table
is easy to reuse elsewhere; hovering a header explains what that
statistic means in your chosen language. The available statistics are:

- **column_name** / **type** - the source column and its inferred data
  type (always shown).
- **min** / **max** - smallest and largest value.
- **sum** - total of the numeric values.
- **mean** / **median** / **std_dev** - average, middle value, and
  standard deviation (numeric columns).
- **range** - largest minus smallest value; **iqr** - the interquartile
  range (q75 minus q25).
- **q25** / **q75** - lower and upper quartiles (numeric columns).
- **mode** / **mode_count** - the most frequent value and how often it
  occurs.
- **not_null** / **null_count** / **null_percent** - counts of present
  and missing values, and the missing share.
- **unique_count** - exact count of distinct values (nulls excluded).
- **distinct_ratio** - unique values divided by total rows.
- **text_len_min** / **text_len_max** - shortest and longest text length
  in characters.
- **total_rows** - row count of the whole table.

## How Min / Max work for text

For numbers, dates, and times, **Min** and **Max** are the smallest and
largest values as you'd expect. For **text** columns the comparison is
"dictionary" order by character code, not by length or meaning:

- It compares character by character, left to right.
- It is case-sensitive, and uppercase letters come before lowercase
  ones, so `"Zebra"` sorts before `"apple"`.
- Digits compare by their character, not their numeric value, so as
  text `"10"` sorts before `"9"` (the character `"1"` comes before
  `"9"`). Numbers stored as text do not sort numerically.

If a column should sort numerically or by date, give it a numeric or
date type (Octa's date inference and the SQL view's `CAST` can help)
rather than leaving it as text.

## Choosing which statistics show

**Settings > Summary** has a checkbox per statistic. Turn off the ones
you don't care about and the Summary tab drops those columns; column_name
and type are always present. The core figures come from a single DuckDB
`SUMMARIZE` pass, plus derived null counts, an exact distinct-value
count, and (only when those statistics are switched on) one extra pass
for sum and text lengths and one per column for the mode.

## Number formatting

Numeric statistics are stored as real numbers, not text, so they follow
the same display settings as the main table and right-align like numbers.
When **thousand separators** are switched on (**Settings > Display**),
figures like sum, total rows, and the counts are grouped, and the chosen
English / European style sets the grouping and decimal marks. A numeric
column's min / max / mode group too; a text column's stay verbatim, as do
the column name and type. Saving or exporting the Summary keeps clean
numbers underneath (no separators baked in).

## Working with the result

The Summary tab is an ordinary table tab: you can sort it, filter it,
copy cells, and export it via **File > Save As**. It is a detached
snapshot with no source path, so it can never overwrite the original
file. Re-run **Analyse > Summary...** after further edits to get a
fresh snapshot.

For a deeper look at a single column, use Value Frequency instead.
"#;

pub(super) const PIVOT: &str = r#"# Pivot / Unpivot

Reshape a table between **long** and **wide** form, the way a spreadsheet
pivot table does. Open it via **Analyse > Pivot / Unpivot...**. The result
always opens in a **new detached tab** - your original table is never
changed. It runs on the table as you currently see it, including unsaved
edits.

## Pivot (long to wide)

Pivot spreads one column's distinct values into new columns. Pick:

- **Spread column** - the column whose values become the new column
  headers (e.g. `month`, producing one column per month).
- **Aggregate** - how to combine the values that fall into each new cell:
  `sum`, `count`, `avg`, `min`, or `max`.
- **of** - the value column being aggregated (e.g. `sales`).
- **Group by** - the identity columns kept as rows (e.g. `region`). Leave
  this empty to let DuckDB use every remaining column.

Example: spread `month`, aggregate `sum` of `sales`, group by `region`
turns a long sales log into a region-by-month grid of totals.

## Unpivot (wide to long)

Unpivot is the reverse: it melts several columns into two columns, a name
and a value. Pick the **columns to unpivot** (at least two), then name the
generated **name column** and **value column**. A wide `region, jan, feb,
mar` table becomes a long `region, name, value` table with one row per
region-month.

## Live preview

While the dialog is open it shows a plain-language sentence of what the
current settings do, plus a small preview table of the first result rows.
To stay fast on big tables the preview runs on a sample of the first 1,000
source rows and shows up to 10 result rows; press **Run** to reshape the
full table.

Powered by DuckDB's `PIVOT` / `UNPIVOT`, so it works on any open table.
"#;

pub(super) const CORRELATION: &str = r#"# Correlation

Measure how strongly the numeric columns in a table move together. Open it
via **Analyse > Correlation...**, pick a method, and press **Compute**. The
result opens in a **new detached tab** - your original table is unchanged.

## Methods

- **Pearson** measures linear association (do the values rise and fall
  together in a straight-line way).
- **Spearman** measures monotonic association by correlating the value
  ranks, so it catches consistent up-or-down relationships that are not
  perfectly straight.

## Reading the result

Every numeric column is correlated with every other numeric column. The
result is a square table: the first column lists each variable, and there
is one further column per variable. Each cell holds a coefficient from
**-1** (perfectly opposite) through **0** (no linear/monotonic relation) to
**+1** (perfectly together); the diagonal is always 1. A pair with too few
overlapping values, or no variation, is left blank. Non-numeric columns are
ignored automatically.
"#;

pub(super) const SCHEMA_EXPORT: &str = r#"# Schema Export

Open via **File > Export schema...** or **F7** (remappable).
The dialog opens on the first target (Postgres DDL); switch between
the seven supported targets with the chip row at the top of the
dialog.

Supported targets:

- **SQL DDL (Postgres)**: CREATE TABLE with double-quoted identifiers.
- **SQL DDL (MySQL)**: CREATE TABLE with backtick identifiers + UNSIGNED / DATETIME / BLOB types.
- **SQL DDL (SQLite)**: CREATE TABLE with INTEGER / REAL / TEXT / BLOB affinity.
- **Pydantic v2**: BaseModel subclass with date / datetime imports.
- **TypeScript interface**: number / string / boolean mappings.
- **JSON Schema** (draft 2020-12): object schema with properties + required.
- **Rust struct**: serde-derived struct with chrono types.

Buttons in the footer:

- **Copy to clipboard**: puts the rendered text on the clipboard.
- **Save as...**: opens a save dialog pre-filled with
  `<source_name>_schema.<ext>`.

Type mapping:

- Octa stores types as Arrow strings ("Int64", "Utf8", "Float64",
  "Date32", "Timestamp(...)", ...). Each target maps them to its
  closest native type.
- Unknown Arrow types fall back to each target's TEXT-equivalent
  with a comment so the output is never silently wrong.

Identifier safety:

- Column names with spaces / hyphens / leading digits get quoted
  (SQL, TypeScript) or sanitised + aliased (Pydantic Field(...,
  alias=...), Rust #[serde(rename = "...")]) so the model still
  round-trips JSON / CSV with the original key.

The active row filter does *not* affect schema export -- only the
column list does.
"#;

pub(super) const ARCHIVE_VIEWER: &str = r#"# Archive Viewer

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

pub(super) const SELECTION_STATS: &str = r#"# Selection Stats

Selecting more than one cell adds a pill to the status bar that
summarises the selection:

- For numeric cells: **Count**, **Sum**, **Avg**, **Min**, **Max**.
- For mixed or non-numeric selections: just **Count**.

Selection sources fall through in the same order the clipboard
uses: a multi-cell selection (Ctrl+Arrow) takes priority, then row
selections, then column selections. Single-cell selections fall
back to the existing Cell / Type info pill instead.
"#;

pub(super) const PINNED_TABS: &str = r#"# Pinned Tabs

Right-click any file-backed tab and pick **Pin tab** to lock it
against accidental closes. Pinned tabs:

- Show a 📌 prefix in the tab label.
- Hide the small × close button.
- Refuse to close on Ctrl+W (and through the unsaved-changes
  prompt). Unpin via the right-click menu first.

## Cross-session persistence

Pinned tabs survive restarts: their file paths are saved in
`settings.toml` under `pinned_tabs` and reopened on next launch.
Files that no longer exist on disk are silently dropped from the
list. Scratch tabs (no source path) cannot be pinned; the menu
entry is greyed out for them.

## Unsaved changes are NOT auto-saved

Pinning does not change save semantics in any way. Closing the
application or closing the tab with unsaved changes still runs the
standard Save / Don't Save / Cancel dialog. The pinned tab reopens
on next launch with whatever is on disk - any unsaved edits from
the previous session are gone if you didn't save them. Save with
Ctrl+S (or Save As) before quitting.
"#;

pub(super) const MARKING: &str = r#"# Color Marking

Right-click a **cell**, **row number**, or **column header** to open the
context menu, then use the **Mark** submenu. Available colors: Red, Orange,
Yellow, Green, Blue, Purple.

The **Edit > Mark** menu, and the **Mark** keyboard shortcut (default
**Ctrl+M**), apply a single color to the **whole current selection**: a row
block, column block, multi-cell selection, or single cell. The shortcut uses
the color set under **Settings > Table > Default mark color** (Yellow by
default).

Mark precedence: cell > row > column. To clear a mark, right-click and choose
**Clear Mark**.
"#;

pub(super) const CONDITIONAL_FORMAT: &str = r#"# Conditional Formatting

Where colour marking is something you apply by hand, conditional formatting
colours cells **automatically** based on their value, like the feature of the
same name in a spreadsheet. Open it via **Columns > Conditional formatting...**.

## Rules

The dialog holds a list of rules. Each rule has four parts:

- **Column** - a specific column, or `(any column)` to test every cell.
- **Operator** - `equals`, `does not equal`, `contains`, `does not contain`,
  `greater than`, `less than`, `greater or equal`, `less or equal`,
  `is empty`, `is not empty`.
- **Value** - the text or number to compare against (ignored by the two
  `empty` operators).
- **Colour** - which of the six mark colours to paint matching cells.

Tick **Aa** on a rule to make its text comparison case-sensitive. The
comparison is numeric when both the cell and the value look like numbers
(so `greater than 100` works as you'd expect), otherwise it compares text.

## How rules combine

Rules are checked from top to bottom and the **first** one that matches a
cell wins (like an if / else-if / else chain), so put your most specific
rules first. Use the **^** / **v** buttons on a rule to move it up or down
and build that order. A manual colour mark on a cell always takes priority
over a conditional rule.

Rules apply live as you edit them and update instantly when you change cell
values. They are **per tab and session-only** - they are not saved with the
file and do not change the data, only how it is shown. **Add rule** appends a
new row; the **x** button removes one; **Clear all** removes them all.

To set a cell **value** (rather than a colour) from conditions, use
**Conditional column** instead - see the Transform Column help.
"#;

pub(super) const VALIDATION: &str = r#"# Data Validation

Data validation flags cells that break a rule you define, painting each
failing cell **red** so problems stand out. Open it via
**Columns > Data validation...**.

## Rules

The dialog holds a list of rules. Each rule has a column (a specific
column, or `(any column)` to check every cell) and a kind:

- **Not empty** - the cell must have a value.
- **In range** - the cell must be a number within an optional **min** and
  **max** (leave a bound blank to leave that side open). A non-numeric
  cell fails.
- **Matches pattern** - the cell text must match a regular expression.
- **Unique** - every value in the column must be distinct; duplicated
  cells fail.
- **Max length** - the cell text must be at most the given number of
  characters.

The footer shows a live count of how many cells currently fail.

## How it behaves

Rules apply live: failing cells are highlighted as soon as you add or edit
a rule, and the highlight updates when you change cell values. Validation
highlighting is **per tab and session-only** - it is not saved with the
file and does not change the data, only how it is shown. A manual colour
mark or a conditional-formatting colour takes priority over the red
validation highlight. **Add rule** appends a new rule; the **X** button
removes one; **Clear all** removes them all.
"#;

pub(super) const TRANSFORMS: &str = r#"# Transform Column

Transform Column reshapes your data with a single click, the way you would
clean up a messy spreadsheet by hand. Open it via
**Data > Transform column...**. Pick an operation, fill in its options, and
press **Apply**. Each transform is undoable (Ctrl+Z), session-only until you
save, and respects read-only mode.

## Operations

- **Split column** - break one column into several. Split on a **delimiter**
  (for example a comma, so `a,b,c` becomes three cells), a **regular
  expression**, or a **fixed width** (every N characters). New columns are
  named after the source with a `_1`, `_2`, ... suffix; rows with fewer parts
  get empty cells.
- **Merge columns** - join two or more columns into one new column with a
  separator you choose (like joining First and Last name with a space).
- **Fill down** / **Fill up** - copy the nearest non-empty value into the
  empty cells above or below it. Handy for un-merging the "only show the
  group name on the first row" style of export.
- **Extract pattern** - pull the first regular-expression match out of each
  cell into a new column (for example `#(\d+)` to grab an order number).
  Cells that don't match are left empty.
- **Replace in column** - find and replace within a single column's cells,
  using Plain, Wildcard, or Regex matching (same modes as the search bar).

Split, Merge, and Extract create new columns; Fill and Replace rewrite the
chosen column in place. For the column-creating operations you can set the
new column name and the insert position (leave either blank for the default
shown as the field hint); for Split the name is used as a base, so the parts
become name_1, name_2, and so on. None of them change column types beyond
producing text, and all changes can be undone before you save.

## Conditional column (if / else-if / else)

**Data > Conditional column...** builds a new column whose value depends on
conditions, like a spreadsheet IF/IFS or a SQL CASE. Add an ordered list of
rules such as "if amount > 100 then high, else if amount > 50 then medium,
else low". Each rule tests one column with an operator (equals, contains,
greater than, is empty, ...) and writes its output value when it matches.

Rules are checked top to bottom and the first match wins (that is the
"else if" behaviour); reorder them with the ^ / v buttons. If no rule
matches, the Else value is used. Outputs that look like numbers become
numeric cells; everything else is text. The result is a new column (name
and position configurable) and is undoable with Ctrl+Z.

This shares its operators with Conditional formatting; the difference is
that conditional formatting colours matching cells, while a conditional
column sets a value.
"#;

pub(super) const ANONYMIZE: &str = r#"# Anonymise Columns

**Data > Anonymise columns...** (Ctrl+Shift+Y) prepares a file for sharing by
masking or scrambling sensitive columns. Add rules, pick a strategy for each,
choose where the result goes, and press Apply. An Apply is a single undo step
(Ctrl+Z reverts the whole operation at once).

## Strategies

- **Hash** - replace each value with a stable hex code. The same value always
  hashes to the same code, so the data stays join-able.
- **Partial mask** - keep the first or last N characters and replace the rest
  with a mask character (for example ***-***-1234). Tick **Same length for
  all** to use a fixed number of mask characters for every cell, so the output
  no longer reveals how long the original value was. Left off, it masks exactly
  the hidden characters.
- **Redact** - replace the whole value with a fixed token ([REDACTED]) or an
  empty (null) cell.
- **Fake** - substitute realistic synthetic data (name, email, city, company,
  phone, UUID). Deterministic, so duplicates stay consistent.

A rule can target several columns; for mask / redact / fake the strategy is
applied to each.

## Hashing: SHA-256 vs BLAKE3

Both produce a 256-bit digest written as 64 hex characters. SHA-256 is the
widely known standard; BLAKE3 is a modern hash that is much faster on large
files. For masking either is fine and the result is equally join-able - pick
SHA-256 for familiarity, BLAKE3 for speed.

By default Octa writes the full 64-character hash. Turn off "Output full hash"
to keep only the first N characters as a shorter ID; the fewer characters, the
higher the (still small) chance two different values share a code.

## Salt

The optional **salt** is mixed into every value before hashing. The same value
plus the same salt always gives the same result, so duplicates stay linked and
a re-run with the same salt re-joins to an earlier export. A non-empty salt
makes the output non-guessable. Null and empty cells always pass through
unchanged.

## Combine columns into one ID

Select several columns in one **Hash** rule to hash them together into one new
column (a pseudonymous key), for example first + last into person_id. A
multi-column hash always creates a new column rather than overwriting.

## Output

- **Replace the columns in place** - overwrite the chosen columns.
- **Add the result as new columns** - keep the originals and append the
  anonymised values (e.g. email_anon).
- **Put a sanitised copy in a new tab** - leave the original untouched.

## Command line and assistant

The same engine is available as octa --anonymize spec.json data.csv (a JSON
spec file lists the rules, salt, and output mode) and as the anonymize MCP /
assistant tool.
"#;

pub(super) const SORTING: &str = r#"# Sorting

Click a column header to sort by that column ascending; click again for
descending, and a third time to clear the sort. Sorting applies to the
filtered view, so search first and then sort.

## Sort by several columns

For a multi-level sort, open **Data > Sort by columns...**. The dialog
holds an ordered list of sort keys, each a column and a direction. The
first key is the primary sort; later keys break ties (so, for example,
sort by department ascending, then by salary descending).

Use the **^** / **v** buttons to reorder the keys, **Add column** for
another key, and **x** to remove one. **Apply** sorts the table in place.
"#;

pub(super) const VIEW_MODES: &str = r#"# View Modes

Switch via the **View** menu. Only modes applicable to the current file are
enabled.

- **Table View** (default): structured tabular display with sorting,
  filtering, and editing.
- **Raw Text**: shows the file content as plain text. For CSV/TSV the toolbar
  exposes Quote / Escape / Delimiter combos and an **Align Columns** toggle
  with per-column coloring. Syntect-based syntax highlighting kicks in for
  source-code extensions (Python, Rust, shell, Terraform, ...) and also for
  JSON, YAML, XML and TOML files; the size cap is configurable under
  **Settings -> Performance**.
- **Markdown View**: rendered markdown for `.md` files. Files open in
  **Preview** mode by default (rendered output only). A toolbar toggle
  switches between Preview / Split / Edit. Split places a TextEdit beside the
  preview for live editing. Links in the preview open in your system browser.
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
"#;

pub(super) const COMPARE_VIEW: &str = r#"# Compare View

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

pub(super) const EPUB_VIEW: &str = r#"# EPUB Reader

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

pub(super) const MAP_VIEW: &str = r#"# Map View

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

pub(super) const CHART_VIEW: &str = r#"# Chart

Plot the active table as a histogram, bar, line, scatter, or box chart.
The chart opens as its own **tab** -- not a mode of the source tab --
so you can have several charts of the same data running at once.

Trigger via **Analyse > Chart** or **F5** (remappable). The entry is
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
  tooltip) and a custom **Color** picker.

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

pub(super) const TABS: &str = r#"# Tabs & Folder Sidebar

Every opened file has a tab, even when only one is open. Hovering a tab
reveals the full file path, useful when several tabs share a file name.

**File > Open Directory...** opens a folder browser docked as a sidebar (left
by default; switch to the right under **Settings > Directory Tree**). Click
any file in the tree to open it in a new tab. **File > Close Directory**
hides the sidebar without touching the open tabs.

By default the sidebar lists only sub-folders and files Octa can open, so a
folder full of unrelated files stays readable. Turn off **Show only openable
files** under **Settings > Directory Tree** to list every file instead.
Files without an extension are hidden while the filter is on.

For multi-table databases (SQLite, DuckDB), a picker dialog lists tables and
their row counts before any data loads.
"#;

pub(super) const SQL_VIEW: &str = r#"# SQL View

The **SQL Query** view exposes the active table to an in-memory DuckDB
connection as a temp table named `data`. Press **Ctrl+Enter** to run the
query under the cursor.

- The editor has line numbers, syntax-aware case conversion (UPPER / lower)
  via right-click, and a chip-style autocomplete row showing matching column
  names and SQL keywords. Disable autocomplete in
  **Settings > SQL > Autocomplete** (on by default).
- Results render under the editor; errors render in red.
- **Ctrl+Shift+E** (default) exports the current SQL result.
- The panel can be docked Bottom (default), Top, Left, or Right via
  **Settings > SQL > Panel position**.

## History and snippets

The SQL toolbar offers two ways to reuse queries:

- **History** is a dropdown listing the recent queries you have run in
  this tab (most recent first). Pick one to load it back into the editor.
  The history is per tab and lasts for the session only.
- **Snippets** opens a manager window for a saved library of named queries
  that persists across sessions. Use **Save current query as snippet...**
  to store the editor content under a **name** and an optional
  **description**; each snippet has **Insert** (load it into the editor)
  and **x** (delete). The window has minimise / maximise / close controls
  and is resizable. Snippets live in `sql_snippets.json` in Octa's config
  directory.

## Mutation highlight

After a mutation query (`INSERT` / `UPDATE` / `DELETE`) that changes the
table, Octa briefly marks the changed cells and any new rows in green so
you can see what the query did. Turn this off, or change how long it stays
(in seconds), under **Settings -> SQL** (**Highlight SQL changes** /
**Highlight duration**). The marks clear themselves automatically.

Each query opens a fresh connection; there is no persistent SQL state
between runs.
"#;

pub(super) const CLI_AND_MCP: &str = r#"# Command-line & MCP

Octa is also a small command-line tool. Run with no flags to launch
the GUI (optionally with file paths to open in tabs); run with one of
the action flags to perform that action and exit:

```
octa --schema FILE                 # print column schema
octa --head FILE [-n N]            # print first N rows (default 20)
octa --convert IN OUT              # convert formats (extension-driven)
octa --sql FILE -q '<query>'       # run a SQL query against FILE
```

Output format is controlled with `-f / --format {tsv|json|csv}` (TSV
default). The action flags are mutually exclusive. `-h` and `--help`
show the same long-form output with worked examples for every action.

## MCP server

`octa --mcp` runs a Model Context Protocol server on stdin/stdout.
Six tools cover roughly the CLI surface plus row counting:

- `read_table(path, limit?, table?)`
- `schema(path, table?)`
- `list_tables(path)`: for multi-table sources (SQLite / DuckDB /
  GeoPackage).
- `count_rows(path, table?)`
- `run_sql(path, query, limit?, table?)`
- `convert(input, output, table?)`

(The full tool set is larger; see the online MCP docs.) Tools also
accept **cloud URLs** (`s3://`, `az://`, `gs://`) wherever they take a
`path`, for both reading and writing, using ambient cloud credentials;
`list_objects` browses a bucket.

Defaults (row limit + per-cell byte cap) are configurable under
**Settings -> MCP**; changes require an `octa --mcp` restart. Every
result-bearing tool exposes a `limit` parameter (pass `0` for
unlimited) and surfaces `truncated` / `total_rows_available` /
`cell_truncated` flags so MCP clients know when there's more.

Add Octa as an MCP server to any compatible client (Claude Desktop,
Claude Code, MCP Inspector) pointing the `command` at the `octa`
binary with `--mcp` as the argument.

Add `--mcp-read-only` alongside `--mcp` for a read-only server: the
file-writing tools (`write_table`, `edit_table`, `convert`) are
dropped, so an agent can read and query but not modify files.
"#;

pub(super) const ASSISTANT: &str = r#"# Assistant

A built-in chat assistant can drive Octa's tools over your open tabs.
Toggle the docked chat panel from **Analyse > Assistant**, the **View**
menu, or **Ctrl+Shift+A**. It is GUI-only.

## Providers

Pick a provider and model in the panel header. Supported backends:
Anthropic, OpenAI, Google Gemini, any OpenAI-compatible endpoint, and
local **Ollama** (no API key needed). Cloud providers need an API key,
entered under **Settings > Chat / Assistant**; keys are read from the
environment, then the OS keyring, then `settings.toml` (in that order).

## What it can access

The assistant sees only your **open tabs** (and the other sheets/tables
of an open workbook or database). It cannot read arbitrary files. It can
also read and list **cloud objects** (s3://, az://, gs://) in buckets you
have saved as a connection under **Settings > Cloud storage**; unsaved
buckets are refused. Writes are confined to the export directory
(**Settings > Chat / Assistant > Export directory**, default ~/Downloads)
unless you give an absolute path. It can read, query (SQL), profile,
convert, chart, and write data through the same tools the MCP server
exposes.

Tool results are capped at **Settings > Chat / Assistant > Result row
limit** (default 200 rows) so a big query can't flood the conversation.
The query still runs over every row; only what the model sees is capped.
When a result is shortened, the assistant tells you how many of how many
rows it got and offers to write the full result to a file or a tab. Tick
**Unlimited** for no cap.

## Editing your data

By default the assistant cannot change your files (Write protection, on
under **Settings > Chat / Assistant**). Ask it to change an open table and
it says so and offers to save a new file in the export directory instead.

Turn Write protection off to let it edit in place:

- Edit the open tab live: add a computed column (a DuckDB expression,
  including window functions like a moving average), insert rows, set
  cells, delete rows, or drop columns. The change shows up in the tab at
  once and Ctrl+Z undoes it. Nothing reaches disk until you save.
- Edit a file on disk that is not open, including adding or dropping a
  column. Adding or removing a column on a DuckDB, SQLite, or GeoPackage
  file is a schema change and also needs Write protection off.

Before the assistant (or a schema-changing database save) overwrites an
existing file, Octa first copies it to a timestamped .bak sidecar next to it
(**Back up before modifying**, on by default, under **Settings > Chat /
Assistant**). Routine manual saves are not backed up.

## Sessions

Conversations are saved automatically as JSON under `chat_sessions/` in
your config directory. Use **New chat** to start fresh and **History**
to reopen or delete past conversations.

## Exporting a conversation

The **Export** button in the panel header saves the current conversation to
a file. The save dialog offers two formats, chosen by the extension you pick:

- **Markdown (.md)**: a readable transcript with your prompts, the
  assistant's replies, every SQL query it ran (in ```sql code blocks), other
  tool calls, and each tool's result (truncated to keep the file small).
- **JSON (.json)**: the exact saved session, identical to the on-disk
  format, for archiving or further processing.

## Saved prompts

The **Prompts** button next to Send opens a small manager window for
reusable prompts. **Save current prompt...** names and stores whatever is
in the input box; each saved prompt has **Insert** (drop it into the
input) and **x** (delete). The window has the usual minimise / maximise /
close controls and is resizable. Prompts persist across sessions in
`chat_prompts.json` in your config directory, the same way SQL snippets do.

## Tool-call audit log

Turn on **Settings > Chat / Assistant > Tool-call audit log** (off by
default) to record every tool the assistant runs - one JSON line per
call (tool name, argument and result byte counts, duration, error flag,
timestamp) appended to `chat_audit/<session>.jsonl` in the config
directory. It records that a tool ran and how big its input/output were,
not the cell contents. Octa warns once at startup when these logs exceed
a size limit (**Warn when logs exceed**, default 10 MB; can be turned
off). Delete the files in `chat_audit/` to reset.

## Privacy

Prompts, a short description of your open tabs, and any tool results are
sent to the provider you chose. To keep everything local, use Ollama or
point the OpenAI-compatible provider at a local model.
"#;

pub(super) const SAVING: &str = r#"# Saving

- **File > Save** writes back to the original file (preserves format and
  settings).
- **File > Save As** lets you save to a new file, optionally in a different
  format.
- Closing a tab or quitting with unsaved changes prompts a confirmation
  dialog (**Save / Don't Save / Cancel**).
- For SQLite / DuckDB sources, saves are diff-based: only changed rows are
  updated, deleted rows are DELETEd, new rows are INSERTed. Schema changes
  (rename / add / drop column) are rejected; do those in another tool.
- If a tab has a per-column **rounding format**, Save asks whether to write
  the rounded values or full precision. The in-memory table keeps full
  precision either way.
- Excel **write** emits a single `.xlsx` sheet (the active tab); there is no
  multi-sheet write even when the source workbook had several sheets.
"#;

pub(super) const SETTINGS_REFERENCE: &str = r#"# Settings Reference

Open **Help > Settings** (default **F3**). Categories are collapsible:

- **Appearance**: font size and family, theme, icon variant, custom font
  path, custom title bar. The chosen theme applies when you press **Apply**.
- **Table View**: row numbers, alternating row colors, negative-number
  highlight, thousand separators + number style (English / European)
  for numeric cells, edit highlight, default mark color, line breaks,
  binary display mode (Binary / Hex / Text).
- **Search & Editor**: default search mode, search result display, search
  history size, tab size.
- **Summary**: a checkbox per statistic the **Analyse > Summary** tab can
  show (Min, Max, Mean, Median, Std dev, quartiles, null counts, unique,
  distinct ratio, total rows). Column and Type are always shown.
- **File-Specific**: column coloring for raw CSV/TSV, "warn before
  un-aligning" guard, "warn on date format change" banner, "trim
  whitespace on load" + "warn on whitespace trim" toggles, "read-only
  mode notice" toggle, notebook output layout.
- **SQL**: panel position, default row limit, autocomplete, editor font,
  mutation-change highlight (on/off + duration)
  (JetBrains Mono / Match UI / System Monospace).
- **MCP**: default row limit (with **Unlimited** toggle) and per-cell
  byte cap for the `octa --mcp` server. Read at server startup, so
  changes require a restart.
- **Chat / Assistant**: provider + model, API keys, temperature, max
  tool iterations, max response tokens, the result row limit (with an
  **Unlimited** checkbox), panel position, export directory, write
  protection, and the tool-call audit log. See the **Assistant**
  section.
- **Cloud storage**: the **Allow writing to cloud storage** switch and
  your saved S3 / Azure / GCS connections (with their credentials). See
  the **Cloud Storage** section.
- **Map**: default mode (Tiles / Geometry only), tile URL template,
  fall-back-to-geometry toggle for offline / blocked tile fetches.
- **Directory Tree**: sidebar position (left / right), and "show only
  openable files" (on by default) to hide files Octa can't open.
- **Shortcuts**: rebind any keyboard shortcut. Conflicting bindings are
  flagged.
- **Performance**: initial-load row cap (streaming readers), syntax-
  highlight size cap (raw editor fallback), the raw view size cap (largest
  file read fully into the raw editor, default 500 MB, with an Unlimited
  toggle), a user-extensible list of file extensions to open as plain
  text, and how many Excel sheets to auto-open.
- **Files**: how many recent files to remember.
- **Window**: initial size, start maximised. The initial size is the
  pixel size of the window when it is *not* maximised. A maximised window
  always fills the screen, so the size only takes effect once you
  un-maximise (or turn "Start maximised" off) - that is why every size
  setting looks identical while the window is maximised.

Settings persist to:

- Linux: `~/.config/octa/settings.toml`
- macOS: `~/Library/Application Support/Octa/settings.toml`
- Windows: `%APPDATA%\Octa\settings.toml`
"#;

pub(super) const SHORTCUTS_INTRO: &str = r#"# Shortcuts

Every action below can be rebound under **Help > Settings > Shortcuts**.
Unbound actions show `(none)`. The bindings shown are the current ones:
"#;

pub(super) const DEDUPE: &str = r#"# Drop Duplicate Rows

Drop Duplicate Rows removes repeated rows from the active table in one
step, the way you would delete duplicate lines in a spreadsheet. Open it
via **Data > Drop duplicate rows...** (Ctrl+Shift+H).

## How it works

Tick the columns that make up the **key**. Two rows count as duplicates
when all their checked columns are equal. With every column ticked
(the default) only exact whole-row repeats are removed; tick just one
column to collapse rows that share that value.

Choose whether to **keep the first** or **keep the last** occurrence of
each key. Apply removes the rest in a single undoable step (Ctrl+Z brings
them all back), and the status bar reports how many rows were removed.

Values are compared as text, so `1` (integer) and `1.0` (float) are not
treated as the same. The same operation is available on the command line
as `octa --dedupe` and as the `drop_duplicates` assistant/MCP tool.
"#;

pub(super) const IMPUTE: &str = r#"# Fill Missing Values

Fill Missing Values replaces empty or null cells in one column using a
strategy you pick, so you don't have to fill gaps by hand. Open it via
**Data > Fill missing values...**.

## Strategies

- **Mean** / **Median** - fill with the average or middle value of the
  column's numbers (numeric columns only).
- **Mode** - fill with the most common value.
- **Constant** - fill with a fixed value you type.
- **Forward fill** - copy the nearest non-empty value from above.
- **Backward fill** - copy the nearest non-empty value from below.

Only empty/null cells are changed; existing values are left alone. Apply
writes the result back as a single undoable step. A strategy that doesn't
fit the data (for example Mean on a text column) shows an inline error and
changes nothing. Also available as `octa --impute` and the `fill_missing`
assistant/MCP tool.
"#;

pub(super) const UNION: &str = r#"# Union Tables

Union Tables stacks two or more open tabs on top of each other into one
new table, like appending several exports of the same shape. Open it via
**Data > Union tables...**.

## How it works

Tick the tabs to combine. Octa builds a **reconciliation plan**: the
result has the union of all their columns. For each merged column you can
keep or drop it and choose its target type. Columns that appear in only
some tables are filled with empty cells for the rest. Mixed numeric types
widen to a common number type; otherwise the column falls back to text.

Apply opens the combined result in a new tab, leaving the sources
untouched. Also available as `octa --union` and the `union_tables`
assistant/MCP tool.
"#;

pub(super) const JOIN: &str = r#"# Join Tables

Join Tables matches rows between two open tabs, like a spreadsheet VLOOKUP
or a SQL JOIN. You need a second table open in another tab first. Open it
via **Data > Join tables...** (Ctrl+Shift+Q).

## How it works

Pick the **left** table and the **right** table, then add one or more
**conditions**. Each condition pairs any column of the left table with any
column of the right table through an operator:

- `=` equal, `<` less than, `<=` less or equal, `>` greater than,
  `>=` greater or equal.

The columns do **not** need the same name, and their **types do not need to
match** - Octa converts both sides to a common type before comparing
(numbers when both are numeric, otherwise text). So you can join a numeric
`id` against a text `ref`, or match rows where one table's date is `>=`
another's. Add several conditions to require all of them (an AND join).

Then pick the join type:

- **Inner** - keep only rows that match.
- **Left** - keep every row of the left table, filling unmatched right
  columns with empty cells.
- **Right** - keep every row of the right table.
- **Full** - keep every row of both.

The matched result opens in a new tab. Joins run through DuckDB, so they are
fast even on large tables.

The command-line `octa --join` and the `join_tables` assistant/MCP tool
join on shared **column names** with equality (`--join-on`); the in-app
dialog is the place for different column names or non-equal operators.
"#;

pub(super) const PARTITION: &str = r#"# Partition by Column

Partition by Column splits the active table into one file per distinct
value of a column, like sorting rows into folders by category. Open it via
**Data > Partition by column...** (Ctrl+Shift+Z).

## How it works

Pick the column to split on and an output folder. Octa writes one file per
distinct value (named after the value) in the format you choose. For
example, partitioning a sales table by `region` gives you `North.csv`,
`South.csv`, and so on.

The original table is not changed. Also available as
`octa --partition-by` and the `partition_table` assistant/MCP tool.
"#;

pub(super) const OUTLIERS: &str = r#"# Detect Outliers

Detect Outliers highlights numeric values that sit far from the rest of
their column, painting each flagged cell **orange** so unusual readings
stand out. Open it via **Analyse > Detect outliers...**.

## Methods

- **IQR (interquartile range)** - flags cells outside
  `[Q1 - k*IQR, Q3 + k*IQR]`. The usual `k` is `1.5`.
- **Z-score (standard deviations)** - flags cells whose value is more than
  `k` standard deviations from the mean. The usual `k` is `3`.

Tick the columns to scan (numeric columns are pre-selected) and set `k`,
then press **Detect**. Columns with fewer than four numbers are skipped.

## What Detect does

Choose under **When done**:

- **Highlight outlier cells** - paints each flagged cell **orange**. This is
  **per tab and session-only**: it never changes the data, only how it is
  shown, and **Clear highlight** removes it. Manual colour marks, conditional
  colours, and validation highlights all take priority over the orange.
- **Add an is_outlier column** - appends a boolean `is_outlier` column that
  is `true` for every row holding at least one flagged value. This is a real,
  undoable edit (Ctrl+Z) you can save, sort, or filter on.

Also available as `octa --outliers` and the `detect_outliers` assistant/MCP
tool (both report the flagged cells).
"#;

pub(super) const PII: &str = r#"# Detect PII

Detect PII scans the table for columns that look like personal data, so
you can find sensitive fields before sharing a file. Open it via
**Analyse > Detect PII...**.

## How it works

Octa weighs two clues for every column:

- the **column header** (does it look like `email`, `first_name`, `gender`,
  `country`, `birthdate`, `ip`, ...?), and
- the **cell values** (how many match a known shape: email, phone, IP
  address, credit card, IBAN, SSN, date, postal code).

This is why fields with no give-away values, like names, gender or country,
are still found from their header, while a plain number column like
`salary` is left alone.

## Confidence

The percentage combines those two clues:

- a strong value pattern on its own reaches at least 60%,
- a matching header on its own reaches 60%,
- the two together score highest (up to 100%).

A column is listed when its best guess is at least 50%. The **Basis** column
tells you which clue drove it: `column name`, `values (N%)`, or both.

**Send to Anonymise** opens the Anonymise dialog pre-filled with one hashing
rule per detected column. Also available as `octa --detect-pii` and the
`detect_pii` assistant/MCP tool, which return the same `confidence`,
`by_name` and `value_match` fields.
"#;

pub(super) const CLEAN_HEADERS: &str = r#"# Clean Headers on Load

Clean Headers on Load is an optional setting that tidies column names the
moment a file opens, turning headers like `First Name` or `E-mail Address`
into lower snake_case identifiers (`first_name`, `e_mail_address`). Enable
it under **Help > Settings > Clean headers on load**.

## What it does

Each header is trimmed, lowercased, and has spaces and punctuation
replaced with single underscores; leading and trailing underscores are
stripped. Duplicate results get a numeric suffix (`name`, `name_2`) so
every column keeps a distinct name. A header that has no usable characters
becomes `column`.

It is off by default, so files load with their original headers unless you
opt in. It pairs naturally with **Trim whitespace on load**.
"#;

pub(super) const DIAGNOSTICS: &str = r#"# Debug & Reports

## The log

Octa always keeps a log, so there is a record when something goes wrong.
There is no switch to turn it on. It lives in a 'logs' subfolder of Octa's
config folder (logs/octa.log), together with crash details (last_crash.txt),
a run-lock marker (running.lock), and any reports you export. Use **Settings >
Diagnostics > Open log folder** to jump straight there.

Octa's own code logs at 'info' level; third-party libraries are kept to
warnings and errors so the log stays readable.

## Size limit and rotation

The live log is capped at about 5 MB. When it reaches the cap, Octa renames it
to octa.log.1 (replacing the previous octa.log.1) and starts a fresh octa.log.
So there are at most two files, about 10 MB total, and the oldest entries are
eventually discarded. The same check runs at start-up, so a restart never
keeps appending past the limit.

## Debug logging (off by default)

Only the extra detail is opt-in. Turn on **Settings > Diagnostics > Debug
logging** to raise Octa's own code from 'info' to 'debug' for more detailed
entries (it applies immediately, no restart). Leave it off for normal use:
debug entries fill the 5 MB cap faster, so the log rotates sooner and keeps
less history. Switch it on while reproducing a bug, then back off.

## After a crash

Octa records failures two ways. A panic handler writes the time, location,
message, and backtrace to last_crash.txt. A run-lock marker catches harder
crashes the handler cannot (a native crash or a killed process): if the marker
is still there at the next launch, the previous run ended uncleanly. Either
way, the next launch offers to export a report.

## Exporting a report

Use **Help > Export debug report...** to write a single text file (in the logs
folder) with your app version, operating system, theme and language, the tail
of the log, the last crash if any, and your settings. Secrets are stripped and
your home folder and username are masked, so it is safe to attach to a GitHub
issue. No cell values or column data are included.
"#;

pub(super) const CLOUD_STORAGE: &str = r#"# Cloud Storage

Browse and open files directly from Amazon S3 (and S3-compatible providers
such as IONOS, MinIO, and Cloudflare R2), Azure Blob Storage, and Google
Cloud Storage. Saving back to the cloud is **off by default** and must be
turned on.

## Add a connection

Open **Settings > Cloud storage** and click **Add connection**:

- **Name** - a label shown in the sidebar.
- **Provider** - S3, Azure Blob, or GCS.
- **Scope** - **Whole bucket** (target one bucket/container), **Path prefix**
  (confine to a folder inside the bucket, e.g. `team-a/`; the browser roots
  there and cannot go above it), or **Account level** (list every
  bucket/container in the account and pick one to browse).
- **Bucket / Container** - the S3 bucket, Azure container, or GCS bucket (not
  shown for an account-level connection).
- **S3 endpoint** - leave empty for real AWS. Set it for an S3-compatible
  provider (IONOS, MinIO, R2, ...); those usually also need **Path-style
  addressing** on, and a local MinIO may need **Allow HTTP**.
- **AWS profile** - a named profile for SSO sign-in (resolved through the AWS
  CLI). Leave empty to use ambient credentials.
- **Storage account** (Azure only).
- **GCP project** / **gcloud account** (GCS account-level only) - GCS buckets
  belong to a **project**, so account-level listing needs the project id
  (empty = your active `gcloud` project) and optionally the gcloud identity
  (email) if you have several logged-in accounts.

### Several accounts or projects

An account-level connection lists one account/project at a time, because each
provider scopes bucket listing differently. To cover several, make one
connection per scope: for **AWS/S3** set a different **Profile** per account;
for **Azure** a different **Storage account**; for **GCS** a different **GCP
project**. Account-level listing needs the provider CLI (`aws` / `az` /
`gcloud`) installed and broader list permissions.

### Credentials

Octa resolves credentials in this order: a **secret you save** on the
connection, then the **ambient** environment (AWS_* variables, a cached SSO
session, Azure CLI login, or Google application-default credentials).

- **S3 / S3-compatible**: save an **Access key ID** + **Secret** for static
  keys, or use a profile / `aws sso login` for AWS SSO.
- **Azure**: save an account key or a **SAS token**, or sign in with the
  Azure CLI.
- **GCS**: uses application-default credentials (`gcloud auth
  application-default login`) or `GOOGLE_*` environment variables.

Saved secrets are stored in your operating system keyring when available,
otherwise in `settings.toml`. **Clear secret** removes a stored secret.

### Public / anonymous buckets

For a **public, read-only** bucket or container, tick **Public / anonymous
access** in the connection form. Octa then skips request signing entirely, so
it opens with no credentials and no sign-in. (Without this, a public Azure
container would redirect to a login and fail.) No secret is needed, and the
sidebar shows the connection as `(public)`.

## Sign in (browser SSO)

A **Sign in** button is only needed for **browser SSO** sign-in, and only
appears for connections that use it. It shells out to the cloud's official CLI:

- S3: `aws sso login` (with `--profile` if set)
- Azure: `az login`
- GCS: `gcloud auth application-default login`

You do **not** need any CLI for static keys, a SAS token, ambient environment
credentials, a GCS service-account key, or a public connection - only for the
in-app browser sign-in. When the CLI is missing, the connection shows a
**"Sign in needs CLI"** note instead of the button (hover it for the full
reason). Octa never implements the OAuth flow itself.

On **Windows**, all three CLIs have native installers (the AWS CLI MSI, the
Azure CLI MSI, the Google Cloud SDK installer); WSL is not required. If your
CLI only lives inside WSL, native-Windows Octa will not see it - install the
CLI on Windows, or use static keys / a SAS token instead.

## Browse and open

Open the sidebar with **File > Cloud connections**. Click a connection to list
its bucket root, expand folders to drill in (listings load in the background
and are cached), and click a file to open it. The file is downloaded to a
temporary copy and opened in a new tab, just like a local file, so every
supported format works. **Refresh** re-lists a connection (for example after
signing in or after the bucket changed).

Use the **Sort** menu next to the Connections header to order files by name,
last-modified date (newest / oldest), or size (largest / smallest). Folders
always sort by name and stay at the top.

## Saving back

By default, cloud-opened files are read-only: pressing **Save** shows a
reminder and does nothing, but **Save As** to a local path always works (and
detaches the tab from the cloud).

To save back to the object, turn on **Allow writing to cloud storage** in
**Settings > Cloud storage**. Then **Save** writes the tab back to its
original object. Uploads run in the background; the status bar reports success
or failure.

The same switch also lets the **assistant** write to the cloud: ask it to save
a result to a cloud URL (e.g. `s3://bucket/out.parquet`) and its write tools
upload it to a bucket you have saved as a connection. The headless MCP server
(`octa --mcp`) writes to cloud URLs too, using ambient credentials; run it with
`--mcp-read-only` to remove every write tool.

## Connection status

Each connection's name carries its provider in brackets - `(S3)`, `(Azure)`,
or `(GCS)`. Under the name the sidebar shows how it authenticates - **Public**,
**Saved keys**, or **Sign-in** - and, once you have expanded it at least once,
whether the bucket was **reachable** (green) or **not reachable** (red). The
status comes from the last listing; it is not a live connection (see below).

## Signing out

A connection that uses **saved keys** shows a **Sign out** button. It removes
that connection's stored credentials from this computer (the same as **Clear
secret** in Settings), after a confirm. This is local only - a browser SSO
session lives in the cloud CLI, not in Octa, so you end that there (for example
`aws sso logout`). A public connection has nothing to sign out of.

## Is it always connected?

No. Object storage is not a persistent session - every list, open, and save is
an independent request. A saved connection is just **configuration** (the
bucket plus how to authenticate), like a bookmark; it stays in the list across
restarts but nothing is "connected" in between. There is nothing to keep open
and nothing that drains while idle.
"#;
