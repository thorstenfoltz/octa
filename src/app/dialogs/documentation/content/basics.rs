//! Getting around: opening files, moving in the grid, editing, saving,
//! sorting, tabs and the shortcut list.
//!
//! One of six topic files split out of `content.rs`, which held all 77 section
//! bodies in a single 4,261-line, 192 KB file. Text moved verbatim; the parent
//! `content/mod.rs` re-exports every constant, so `documentation::sections()`
//! is untouched.
//!
//! ASCII only: egui's bundled font renders typographic punctuation as tofu.

pub const GETTING_STARTED: &str = r#"# Getting Started

> **Full documentation online:** <https://thorstenfoltz.github.io/octa/>
>
> This dialog is the short version, always matching the build you are
> running. The site carries the same material plus screenshots, the CLI
> reference, the MCP tool pages and the settings reference.

Open a file from **File > Open** (or **Ctrl+O**), pick one or more from the
**File > Recent Files** submenu, or pass paths on the command line:

```
octa path/to/file.parquet other.csv
```

Multiple files open into separate tabs.

## A toolbar too wide for the window

On a small screen the toolbar holds more than fits. It scrolls sideways: point
at it and use the mouse wheel, or drag the slim scrollbar under the row. The
window buttons on the right (with a custom title bar) keep their place and are
never pushed off the edge.

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

### What a save keeps

Writing a format is not the same as reproducing the file you opened.
Octa works on a flat table of typed cells, and what a format carries
beyond that survives only where reader and writer were built as a pair:

- Excel keeps cell formatting and formulas (both opt-in under
  **Settings > Files > Write options**), notebooks keep their cell
  outputs, Markdown / source / plain text are written back line for
  line, and SQLite / DuckDB / GeoPackage are edited in place so
  untouched tables, views and indexes stay as they were.
- JSON, JSONL, XML, TOML and YAML are nested documents, and a table is
  not nested. Reading flattens nested objects into dotted column names
  (`address.city`); saving writes a flat array of records, or for XML a
  generic `<data><row>` document. Inspect nested files here, edit their
  structure in a text editor.
- Excel always writes `.xlsx` structure whatever extension it read, and
  SPSS variable / value labels are not carried into a `.sav` Octa writes.

## Read-only formats

- SAS (`.sas7bdat`)
- R Datasets (`.rds`, `.rdata`, `.rda`)
- HDF5 (`.h5`, `.hdf5`, `.hdf`)
- NetCDF v3 (`.nc`)
- NumPy (`.npy`, `.npz`)
- MessagePack (`.msgpack`, `.mpk`)
- BSON (`.bson`)
- EPUB (`.epub`)
- HTML (`.html`, `.htm`): every `<table>` on the page opens as its own
  table, the way workbook sheets do; a `<caption>` names it. `rowspan` and
  `colspan` expand into repeated cells, and a leading row of `<th>` becomes
  the header. **File > Open URL** fetches a page and hands it straight
  here, so a Wikipedia article opens as its tables. To read the markup
  itself, use **View > Reopen as > Text**.
- SQL dump (`.sql`, opt-in): a `.sql` opens as text, as it should. To read a
  `mysqldump`, `pg_dump` or `sqlite3 .dump` as its tables, ask for it by name
  with **File > Open as > SQL dump** or **View > Reopen as > SQL dump**;
  opening a `.sql` that turns out to hold CREATE TABLE and INSERT INTO says so
  in the status bar and points at that entry. Octa
  scrubs the dialect off each statement and replays the file into a scratch
  database, then offers the same table picker a database file does. Statements
  no engine but the original could run are skipped quietly; when one carrying
  schema or data fails, a banner says how many and quotes the first. Capped at
  512 MB, since the replay happens in memory.
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

Each tab is labelled `workbook.xlsx - Sheet name` so sheets of one file can be
told apart; a table picked out of a SQLite or DuckDB file is labelled the same
way. Renaming the tab still overrides the label.

## Excel formulas

A cell computed by a formula shows the value Excel last calculated, and the
formula behind it comes along: **hover the cell** to see it, or open the
**Record** view (F4), which lists it beside the field.

Saving writes values, not formulas. Turn on **Settings > Files > Write
options > Excel > Keep Excel formulas when saving** to write the formula back
instead. It is off by default for a reason worth knowing: Excel recalculates a
formula when it opens the file, so the number in the saved workbook can end up
different from the one Octa showed you.

Two things retract a formula, whatever the setting says, because keeping them
would put a wrong answer in the file:

- **A cell you edited.** Your value is what you meant; a formula that would
  recompute over it is no longer true.
- **A table whose rows or columns you moved, added or deleted.** A formula says
  `=B2*C2`, and inserting a row changes what `B2` points at. Octa cannot
  rewrite the references, so it drops every formula rather than keep ones that
  now mean something else.

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

pub const ONLINE_DOCS: &str = r#"# Online Documentation

The full documentation lives at:

<https://thorstenfoltz.github.io/octa/>

It is built from the `docs/` folder of the repository and published on
every release, so it always describes a released version. This in-app
dialog ships inside the binary and therefore always matches exactly the
build you are running - if the two ever disagree, this one is right about
your build and the site is right about the latest release.

What is on the site and not here:

- Screenshots of every dialog and view.
- The complete command-line reference, one page per flag, plus the man
  page.
- The MCP tool reference, one page per tool, with request and response
  examples.
- The settings reference with every TOML key.
- Installation and packaging notes for Linux, Windows and macOS.

Source repository: <https://github.com/thorstenfoltz/octa>
"#;

pub const NAVIGATION: &str = r#"# Navigation & Selection

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

## Split view

**View > Split view** cuts the table into two bands, one above the other,
so you can keep row 12 in sight while you read row 900,000.

**View > Split side by side** does the same the other way round: two bands
next to each other, so you can read the first column beside the last one.
Freezing columns solves part of the same problem by pinning the leading
columns; this frees both bands to sit anywhere in the table.

The two entries are checkboxes and are mutually exclusive: clicking the one
that is not showing switches orientation in a single click, and clicking the
active one turns the split off.

Every band scrolls on its own, both up and down and left and right. Two bands
showing the same cells would be no use, so nothing is locked together. Each
band has its own scrollbars, and dragging one moves that band alone. Drag a
divider to resize the bands either side of it; no band can be squeezed below
about 80 pixels.

Hold **Alt** and the wheel moves every band at once, for walking several bands
down the table in step. Alt+Shift+wheel does it sideways. Each band still stops
at its own end. Ctrl+wheel zooms and Shift+wheel scrolls sideways, so Alt is
the one left for this.

**View > Add pane** cuts one more band out of the split, up to 6, and
**Remove pane** takes one away down to two. Both are greyed out until the
view is split, and at their limits. Changing the count re-spaces the dividers
evenly. The orientation entries keep the count, so four stacked bands become
four side-by-side ones in one click.

Every band is the same table: same columns, widths, frozen band, filters,
sort, marks and edits, and the selection spans all of them, so a range from
row 12 down to row 900,000 is one selection.

The keyboard and the mouse wheel act on the band the pointer was last over,
so one arrow press moves one selection and one wheel notch scrolls one band.
The split is per tab and session-only, like column widths, and applies to
the table view only.
"#;

pub const EDITING: &str = r#"# Editing & Undo/Redo

- **Double-click** a cell to start editing; the current text is selected so
  you can type to replace it, or click to position the cursor.
- Click outside the cell or press **Tab** / **Enter** to confirm; **Escape**
  cancels.
- **Undo** (Ctrl+Z) and **Redo** (Ctrl+Y) cover cell edits, row/column
  insert/delete/move, and colour marks. Both are also available in the **Edit**
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

## Write options

**Settings > Files > Write options** controls how Octa writes files, as
opposed to what it writes. It holds one expander per format, since no
control in it applies to more than one: **Parquet** gets compression,
rows per row group, dictionary encoding and column statistics; **CSV /
TSV** gets the delimiter, quoting, line endings and whether to write a
header row; **Excel** gets the two `.xlsx` switches, include formatting
and keep formulas. Open the group for the format you are writing.

Parquet is written with **zstd** compression by default. Uncompressed
Parquet is only about 1.6x smaller than the same data as CSV, where zstd
reaches roughly 5x, and it costs about 1% more write time and 3% more
read time to get there. Pick `uncompressed` here if you need it; every
codec stays available.

The CSV delimiter applies to CSV files. Left at a comma, each file keeps
the delimiter it was opened with, so a semicolon file stays a semicolon
file. A `.tsv` is always written tab-separated whatever the setting says,
since that is what the format means; set a different delimiter and save
as `.csv` if you want it.

Save As uses these settings, since it is the operating system's file
picker and has nowhere to put controls; the Batch convert dialog shows
the same controls in a **Write options** expander that applies to that
run only. On the command line the two Parquet knobs are `--compression`
and `--row-group-size`, and leaving them off uses these same settings, so
a conversion in the terminal writes the same file the app would. That
covers the CSV knobs too: a `--convert` in the terminal picks up the
delimiter, quoting, line endings and header row saved here.

The File internals tab is the other half of this: it shows how an
existing file was written, including whether it is compressed at all.

## Whitespace trimming on load

By default Octa strips leading/trailing whitespace from string cells
**and column titles** when a file opens (interior spaces are kept), and
shows a banner listing which columns changed. Both the trimming and the
banner can be turned off under **Settings > File-Specific**.

## European numbers on load

A German or French export writes amounts as `1.234,56`. Octa recognises
both that and the English `1,234.56` when a file opens, and reads such
columns as real numbers, so they sort by size, sum, chart and take part
in SQL arithmetic instead of sitting there as text.

The decision is made per column, not per cell, because one value on its
own can be undecidable: `1,234` is one thousand two hundred and thirty
four in a European file and one point two three four in an English one.

- Columns that can only be read one way are converted, and a banner
  names them. **Okay** keeps the conversion, **Dismiss** puts the
  original text back.
- Columns that could go either way raise a small dialog with sample
  values and three answers: European, English, or leave as text.

Groups after the first must be exactly three digits, which is why
`31.12.2024` is never read as a number and stays a date.

Saving an edited file is described under **Saving**.
"#;

pub const FORMULAS: &str = r#"# Formulas

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

pub const SORTING: &str = r#"# Sorting

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

pub const TABS: &str = r#"# Tabs & Folder Sidebar

Every opened file has a tab, even when only one is open. Hovering a tab
reveals the full file path, useful when several tabs share a file name.

**Rename a tab.** Right-click a tab and choose **Rename tab...** (or press
Ctrl+Alt+T) to give it any label you like. This changes only what the tab shows; the
file path and the name on disk are unchanged, and hovering the tab still reveals
the full path. Clear the name to go back to the file name.

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

pub const PINNED_TABS: &str = r#"# Pinned Tabs

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

pub const PDF_EXPORT: &str = r#"# Export to PDF

**File > Export to PDF...** prints what the active tab is showing to a
paginated PDF: the grid, the Summary tab, the Data Quality Report and its
section tabs, a comparison, or any other result tab. They are all tables, so
they all export the same way. The same entry sits on a tab's right-click menu,
which exports that tab.

## What ends up on the page

Exactly what you can see, and nothing you cannot: the rows the current filter
leaves, in the sort order on screen; the visible columns in their current
order; colour marks and conditional-formatting colours in the same palette the
grid paints; and unsaved cell edits, because the export reads the cells the way
the grid does. Values print as they are stored, without the thousands
separators the grid can add, the same as every other export.

## Pagination

Nothing is truncated to make the table fit. A long table pages down, a wide one
pages across, in reading order: all the columns of the first band of rows, then
the next band.

- The header row repeats on every page.
- Frozen columns repeat on every page across, so a page of columns 40 to 48
  still tells you which record you are looking at. Freeze them first
  (right-click a column header > Freeze columns up to here).
- A footer on every page carries the file name, the page number and the row
  range, plus the column range when the table needed more than one page across.
- A cell too long for its column is cut with an `...`; the width is measured
  from the first 200 visible rows.

## The dialog

- **Page size**: A4 or Letter.
- **Orientation**: portrait or landscape. Landscape fits more columns on a
  page, portrait more rows.
- **Describe the view on the first page**: adds a line under the title naming
  the active filter and the row and column counts. On by default.

The dialog tells you how many pages the export will be before you write it.
There is no row or column cap, so a five-million-row table really will produce
tens of thousands of pages: filter first, and let the page count tell you.

The title is the file name, or the tab label for a result tab. The document
chrome is English, like the HTML report, because these are files that travel
outside the app.

The Report is a different document, with charts and per-column sections,
written as HTML. To get that as a PDF, open it in your browser and print to
PDF; the browser lays out its charts properly.
"#;

pub const SAVING: &str = r#"# Saving

- **File > Save** writes back to the original file (preserves format and
  settings).
- **File > Save As** lets you save to a new file, optionally in a different
  format.
- Closing a tab or quitting with unsaved changes prompts a confirmation
  dialog (**Save / Don't Save / Cancel**).
- **If something else changed the file** after you opened it, Save stops and
  asks instead of overwriting it: **Save anyway** writes your version over
  it, **Reload** rereads the file and drops your unsaved edits (as Ctrl+R
  does), **Cancel** touches nothing so you can Save As elsewhere and compare.
  Octa notices by remembering the file's modification time and size. Only
  Save is guarded; Save As writes where you point it. Auto-save skips such a
  tab rather than raising the prompt.
- For SQLite / DuckDB sources, saves are diff-based: only changed rows are
  updated, deleted rows are DELETEd, new rows are INSERTed. Schema changes
  (rename / add / drop column) are rejected; do those in another tool.
- If a tab has a per-column **rounding format**, Save asks whether to write
  the rounded values or full precision. The in-memory table keeps full
  precision either way.
- Excel **write** emits a single `.xlsx` sheet (the active tab); there is no
  multi-sheet write even when the source workbook had several sheets.

**Formatting in Excel files.** Colour marks, conditional-formatting colours,
frozen columns and per-column number formats are display-only everywhere else,
but `.xlsx` can hold all four. Carrying them across is off by default; the
switch is **Settings > Files > Write options > Excel > Include formatting in
Excel files**. Saving a tab that carries any of the four to `.xlsx` asks once
anyway: **Include formatting**, or **Plain data** for the values alone. Tick **Do not
ask again** to store the answer as the Settings default. No other format,
`.ods` included, is affected. This travels one way: opening the saved workbook
back in Octa reads the values alone, so a round-trip loses the formatting.

Most conditional rules cross over as live Excel rules and keep working when you
edit the sheet there. Three kinds cannot, and are painted onto the cells
matching at save time instead, so their colours stay put when the cell changes
in Excel: ordering comparisons over text (Excel orders text by locale
collation, Octa by plain character order), case-sensitive equality or contains
rules (Excel's own are always case-insensitive), and every rule after the first
painted one (a live Excel rule always beats a cell's own fill regardless of
order, so painting the remainder is what keeps the file agreeing with the
screen). One accepted difference: an equality rule whose value looks like a
number exports as a numeric comparison, while Octa compares equality as text,
so a cell holding 42.0 against a rule value of 42 can colour in Excel but not
in Octa.

**Auto-save.** Turn on **Settings > Files > Auto-save** and set an interval in
minutes (minimum 1). Every interval, Octa writes each open tab that has unsaved
changes and already lives as a file on disk. It is off by default. It never
interrupts you: tabs never saved to disk, cloud tabs when cloud writing is off,
and saves that would normally ask a question (a rounding format, an `.xlsx` tab
carrying formatting, or a database schema change) are skipped quietly. When it writes something, the status bar
shows a brief "Auto-saved N files" note.
"#;

pub const SHORTCUTS_INTRO: &str = r#"# Shortcuts

Every action below can be rebound under **Help > Settings > Shortcuts**.
Unbound actions show `(none)`. The bindings shown are the current ones:

Click **Record** on a row and press the combination you want. While Octa is
waiting for that press, the keys do nothing else: recording Ctrl+S records
Ctrl+S, it does not save the file. Esc stops recording.

Two actions can never share a combination. If the one you press is already
taken, Octa says which action holds it and offers **Take it over**: the key
moves to the action you are recording and the previous owner is left unbound.
Nothing is written until you click Apply.
"#;

pub const SELECTION_STATS: &str = r#"# Selection Stats

Selecting more than one cell adds a pill to the status bar that
summarises the selection:

- For numeric cells: **Count**, **Sum**, **Avg**, **Min**, **Max**.
- For mixed or non-numeric selections: just **Count**.

Selection sources fall through in the same order the clipboard
uses: a multi-cell selection (Ctrl+Arrow) takes priority, then row
selections, then column selections. Single-cell selections fall
back to the existing Cell / Type info pill instead.
"#;

pub const TABLE_TOOLS: &str = r#"# Table Tools

A few quick utilities for reshaping and tidying the active table.

**Transpose** (**Analyse > Transpose...**). Swaps rows and columns into a new tab:
the original column names become the first column, and each original row becomes
a column. Everything is shown as text. Limited to tables of at most 1000 rows,
since each row becomes a column.

**Compare rows** (**Analyse > Compare rows...**). Puts the rows
you picked side by side, one column each, so you can see where two records, or
ten, actually disagree. Either gesture picks them: select the rows (click a row
number, then Ctrl+click or Shift+click the others), or mark them (right-click a
row number > Mark, or Ctrl+M). The selection wins whenever it holds two or more
rows, so a stray click cannot replace a set of marks you built on purpose; with
fewer than two selected, the marks are used. The entry stays greyed out until
either gesture has picked two rows, because one row differs from nothing. The
result tab holds one row per original column, with a `differs` flag in front:

    column   differs  row 12   row 4711
    city     no       Aachen   Aachen
    amount   yes      12.50    13.10

Every field answers yes or no, so a blank cell never has to be read as "not
checked". Values are compared exactly as they are displayed, and only whole-row
marks pick a row: marked cells and marked columns are ignored.

**Random sample** (**Analyse > Random sample...**). Opens a new tab with a number
of rows you choose, picked at random from the active table. Handy for eyeballing
a fair cross-section of a big file without scrolling all of it. If you ask for
more rows than the table has, you get them all.

**Tidy up** (**Data > Tidy up...**). Cleans the current table in one undoable
step: trim stray spaces from cells and column titles, and optionally tidy the
column names to snake_case. A single Undo reverts the whole thing.

**Clickable links.** When a cell holds a web address (http/https), it is shown
as an underlined link. **Ctrl+click** opens it in your browser; a plain click
still selects the cell. Turn this off with **Settings > Table View > Clickable
web links**.
"#;
