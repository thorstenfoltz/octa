//! In-app documentation. Categorized into sections so the dialog can offer a
//! sidebar nav (left) + content pane (right), mirroring the structure of the
//! Settings dialog. The shortcut table is generated from the user's current
//! bindings each time the dialog opens, so it never drifts from behavior.

use eframe::egui;

use octa::ui;
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::OctaApp;
use crate::view_modes::markdown::render_pulldown;

const SIDEBAR_WIDTH: f32 = 180.0;

/// Build the Markdown shortcut table rendered in the Shortcuts section.
fn build_shortcut_doc_table(shortcuts: &ui::shortcuts::Shortcuts) -> String {
    use strum::IntoEnumIterator;
    let mut s = String::from("| Shortcut | Action |\n|----------|--------|\n");
    for action in ui::shortcuts::ShortcutAction::iter() {
        let combo = shortcuts.combo(action);
        s.push_str(&format!("| {} | {} |\n", combo.label(), action.label()));
    }
    s
}

/// Returns the ordered list of documentation sections. The Shortcuts section
/// embeds the live key-binding table; all other sections are static.
fn sections(shortcuts: &ui::shortcuts::Shortcuts) -> Vec<(&'static str, String)> {
    let shortcut_table = build_shortcut_doc_table(shortcuts);
    vec![
        ("Getting Started", GETTING_STARTED.to_string()),
        ("Navigation & Selection", NAVIGATION.to_string()),
        ("Editing & Undo/Redo", EDITING.to_string()),
        ("Formulas", FORMULAS.to_string()),
        ("Search & Replace", SEARCH.to_string()),
        ("Multi-search", MULTI_SEARCH.to_string()),
        ("Column Filter", COLUMN_FILTER.to_string()),
        ("Column Tools", COLUMN_TOOLS.to_string()),
        ("Value Frequency", VALUE_FREQUENCY.to_string()),
        ("Find Duplicates", FIND_DUPLICATES.to_string()),
        ("Schema Export", SCHEMA_EXPORT.to_string()),
        ("Archive Viewer", ARCHIVE_VIEWER.to_string()),
        ("Selection Stats", SELECTION_STATS.to_string()),
        ("Pinned Tabs", PINNED_TABS.to_string()),
        ("Color Marking", MARKING.to_string()),
        ("View Modes", VIEW_MODES.to_string()),
        ("Compare View", COMPARE_VIEW.to_string()),
        ("EPUB Reader", EPUB_VIEW.to_string()),
        ("Map View", MAP_VIEW.to_string()),
        ("Chart", CHART_VIEW.to_string()),
        ("Tabs & Folder Sidebar", TABS.to_string()),
        ("SQL View", SQL_VIEW.to_string()),
        ("Command-line & MCP", CLI_AND_MCP.to_string()),
        ("Saving", SAVING.to_string()),
        ("Settings Reference", SETTINGS_REFERENCE.to_string()),
        (
            "Shortcuts",
            format!("{}\n\n{}", SHORTCUTS_INTRO, shortcut_table),
        ),
    ]
}

pub(crate) fn render_documentation_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_documentation_dialog {
        return;
    }
    let mut window = egui::Window::new("Documentation")
        .title_bar(false)
        .collapsible(false);
    window = match app.documentation_size {
        DialogSize::Maximized => window.fixed_rect(ctx.content_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => window.resizable(true).default_size([900.0, 600.0]),
    };
    let minimized = app.documentation_size == DialogSize::Minimized;
    window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Documentation").strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if draw_window_controls(ui, &mut app.documentation_size) {
                    app.show_documentation_dialog = false;
                }
            });
        });
        ui.separator();

        if minimized {
            return;
        }

        let entries = sections(&app.settings.shortcuts);
        if app.docs_active_section >= entries.len() {
            app.docs_active_section = 0;
        }

        ui.horizontal_top(|ui| {
            // --- Sidebar nav ---
            ui.allocate_ui_with_layout(
                egui::vec2(SIDEBAR_WIDTH, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(SIDEBAR_WIDTH);
                    egui::ScrollArea::vertical()
                        .id_salt("docs_sidebar_scroll")
                        .show(ui, |ui| {
                            for (idx, (title, _)) in entries.iter().enumerate() {
                                let is_active = idx == app.docs_active_section;
                                let resp = ui.selectable_label(is_active, *title);
                                if resp.clicked() {
                                    app.docs_active_section = idx;
                                }
                            }
                        });
                },
            );
            ui.separator();
            // --- Content pane ---
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("docs_content_scroll")
                    .show(ui, |ui| {
                        let body = &entries[app.docs_active_section].1;
                        let cap = ui.available_width().clamp(200.0, 900.0);
                        ui.set_max_width(cap);
                        render_pulldown(ui, body);
                    });
            });
        });
    });
}

const GETTING_STARTED: &str = r#"# Getting Started

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

## Read-only formats

- SAS (`.sas7bdat`)
- R Datasets (`.rds`, `.rdata`, `.rda`)
- HDF5 (`.h5`, `.hdf5`, `.hdf`)
- NetCDF v3 (`.nc`)
- EPUB (`.epub`)
- GeoJSON (`.geojson`)

When saving, the original format and settings (e.g. CSV delimiter) are
preserved. Database writes only update changed rows and reject schema
changes; rename or add columns in another tool first.
"#;

const NAVIGATION: &str = r#"# Navigation & Selection

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

const EDITING: &str = r#"# Editing & Undo/Redo

- **Double-click** a cell to start editing; the current text is selected so
  you can type to replace it, or click to position the cursor.
- Click outside the cell or press **Tab** / **Enter** to confirm; **Escape**
  cancels.
- **Undo** (Ctrl+Z) and **Redo** (Ctrl+Y) cover cell edits, row/column
  insert/delete/move, and color marks. Both are also available in the **Edit**
  menu and remappable in **Settings > Shortcuts**.

Structural edits:

- **Edit > Insert Row** adds a new empty row below the selected cell.
- **Edit > Insert Column** opens a dialog to add a column (name + type).
- **Edit > Delete Row / Delete Column** removes the selected one(s).
- **Edit > Move Row Up/Down** and **Move Column Left/Right** reorder data.
- **Edit > Discard All Edits** reverts all unsaved changes.
- **Drag a column header** to reorder columns.
- **Double-click a column header** to rename it inline.
- **Right-click a column header** to change the column data type.

Saving an edited file is described under **Saving**.
"#;

const FORMULAS: &str = r#"# Formulas

Cells support simple Excel-like formulas starting with **=**.

- **Cell references**: A1, B2, AA1, etc. (column letter + 1-based row number;
  the column letter appears in each header).
- **Operators**: `+`, `-`, `*`, `/`.
- **Parentheses**: `(A1 + B1) * 2`.
- **Numeric literals**: `=A1 * 1.5`.

When inserting a column via **Edit > Insert Column**, you can type a formula
into the **Formula** field. The formula is treated as a row-1 template and
applied to every row (e.g. `=A1+B1` becomes `=A3+B3` on row 3).

Division by zero leaves the cell empty.
"#;

const SEARCH: &str = r#"# Search & Replace

The toolbar search box filters rows in real time. Only rows containing a
match are shown. Three modes (selectable in the dropdown next to the box):

- **Plain**: case-insensitive substring.
- **Wildcard**: `*` matches any sequence, `?` matches one character.
- **Regex**: full regular expression syntax.

**Ctrl+F** focuses the search box from anywhere; **Ctrl+H** opens the
**Find & Replace** bar above the table:

- **Next** replaces the first match found.
- **All** replaces every match across visible rows.

**Escape** closes the replace bar.
"#;

const MULTI_SEARCH: &str = r#"# Multi-search

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
Failures no longer hide results from files that searched fine.

Press **Cancel** to stop a running directory scan at the next file
boundary. Whatever hits were already collected stay in the panel.
"#;

const COLUMN_FILTER: &str = r#"# Column Filter

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

const COLUMN_TOOLS: &str = r#"# Column Tools

## Hide and show columns

Right-click any column header and pick **Hide column** to remove it
from the view. Hidden columns are still part of the table on disk:
Save and Save As both write them out. Use **Edit > Show hidden
columns** to bring everything back at once. This is a per-tab,
session-only setting; closing the tab or reopening the file clears
the hidden set.

## Copy column name(s)

Right-click any column header and pick **Copy column name(s)** to
copy the header text to the clipboard. If you have multiple columns
selected (Ctrl-click their headers) and right-click one of them, all
selected names are joined with newlines. Useful for building SQL
SELECT lists or scripts from Octa's view of the file.
"#;

const VALUE_FREQUENCY: &str = r#"# Value Frequency

Open via the column-header right-click **Value frequency...** entry, or
**Ctrl+Shift+I** (remappable). The dialog lists the most common values
in one column, ranked by count.

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

## Numeric binning

For numeric columns, a **Bin numeric values (Sturges)** checkbox groups
the values into `ceil(1 + log2(n))` buckets (clamped to 5..30). Useful
for getting a quick histogram-shaped read on a column. Non-numeric
columns hide the checkbox.

NaN, +Inf, and -Inf show up as separate rows alongside the bins so
type drift is visible.

## Acting on a row

Right-click a row (when binning is off) for:

- **Copy value** - the raw value to the clipboard.
- **Filter table to this value** - adds a column filter restricting
  the active table to rows where this column equals the picked value.

The bottom **Copy as TSV** button copies the whole visible table as
`<column>\tcount\tpercent` lines.
"#;

const FIND_DUPLICATES: &str = r#"# Find Duplicates

Open via **Search > Find duplicates...** or **Ctrl+Shift+D** (remappable).
A modal lists every column with a checkbox — tick the ones you want
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
- A row whose key only matches itself is not a duplicate — results
  always come in pairs or larger groups.
- Hashing is text-based, so `Int(1)` and `Float(1.0)` render as `"1"`
  vs `"1.0"` and therefore do *not* dedupe. Change the column type
  first if you want them to.
- If no duplicates are found, the status bar reports it and the
  active table is unchanged.

The dialog seeds the key with whatever column is currently selected,
so Ctrl+Shift+D → Apply is the fastest path for a one-column dedupe
check.
"#;

const SCHEMA_EXPORT: &str = r#"# Schema Export

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

const ARCHIVE_VIEWER: &str = r#"# Archive Viewer

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

const SELECTION_STATS: &str = r#"# Selection Stats

Selecting more than one cell adds a pill to the status bar that
summarises the selection:

- For numeric cells: **Count**, **Sum**, **Avg**, **Min**, **Max**.
- For mixed or non-numeric selections: just **Count**.

Selection sources fall through in the same order the clipboard
uses: a multi-cell selection (Ctrl+Arrow) takes priority, then row
selections, then column selections. Single-cell selections fall
back to the existing Cell / Type info pill instead.
"#;

const PINNED_TABS: &str = r#"# Pinned Tabs

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
on next launch with whatever is on disk — any unsaved edits from
the previous session are gone if you didn't save them. Save with
Ctrl+S (or Save As) before quitting.
"#;

const MARKING: &str = r#"# Color Marking

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

const VIEW_MODES: &str = r#"# View Modes

Switch via the **View** menu. Only modes applicable to the current file are
enabled.

- **Table View** (default): structured tabular display with sorting,
  filtering, and editing.
- **Raw Text**: shows the file content as plain text. For CSV/TSV the toolbar
  exposes Quote / Escape / Delimiter combos and an **Align Columns** toggle
  with per-column coloring. Syntect-based syntax highlighting kicks in for
  source-code extensions (Python, Rust, shell, Terraform, …); the size cap
  is configurable under **Settings → Performance**.
- **Markdown View**: rendered markdown for `.md` files. A toolbar toggle
  switches between Preview / Split / Edit. Split places a TextEdit beside the
  preview for live editing.
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
"#;

const COMPARE_VIEW: &str = r#"# Compare View

Compare two files side-by-side. Triggered in three ways:

- **View → Compare with…**: opens a file picker; the active tab is the
  left side, the picked file is the right.
- **Right-click a tab → Compare with active tab**.
- The **Compare selected tabs** shortcut (default **F9**, remappable) when
  exactly one tab is **Ctrl-clicked** as the right side.

Two sub-modes toggle in the Compare toolbar:

- **Text Diff**: git-style line-by-line diff of the raw text content,
  rendered with `+` / `-` / `~` markers. Has a 500 ms timeout against
  pathologically slow inputs.
- **Row Hash Diff**: hash the user-picked columns per row (BLAKE3, fast
  and stable). Rows bucket into **Left-only**, **Right-only**, **Shared**.
  Each bucket is expandable and shows the actual cell content (capped at
  50 rows displayed per bucket). With no columns picked, every column is
  hashed; only the first 8 columns are shown to keep rendering snappy.

Cross-format comparison works because hashing sees only the textual
representation of each cell.
"#;

const EPUB_VIEW: &str = r#"# EPUB Reader

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

const MAP_VIEW: &str = r#"# Map View

For `.geojson` files. The Map view is the default; the Table view is
still available with one row per feature, a `__geometry` column holding
the WKT representation, and one column per property.

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
fetch failure" toggle live under **Settings → Map**. For production
deployments please honour the
[OSM tile-usage policy](https://operations.osmfoundation.org/policies/tiles/)
or point at a self-hosted or commercial tile provider.
"#;

const CHART_VIEW: &str = r#"# Chart

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

const TABS: &str = r#"# Tabs & Folder Sidebar

Every opened file has a tab, even when only one is open. Hovering a tab
reveals the full file path, useful when several tabs share a file name.

**File > Open Directory…** opens a folder browser docked as a sidebar (left
by default; switch to the right under **Settings > Directory Tree**). Click
any file in the tree to open it in a new tab. **File > Close Directory**
hides the sidebar without touching the open tabs.

For multi-table databases (SQLite, DuckDB), a picker dialog lists tables and
their row counts before any data loads.
"#;

const SQL_VIEW: &str = r#"# SQL View

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

Each query opens a fresh connection; there is no persistent SQL state
between runs.
"#;

const CLI_AND_MCP: &str = r#"# Command-line & MCP

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

Defaults (row limit + per-cell byte cap) are configurable under
**Settings → MCP**; changes require an `octa --mcp` restart. Every
result-bearing tool exposes a `limit` parameter (pass `0` for
unlimited) and surfaces `truncated` / `total_rows_available` /
`cell_truncated` flags so MCP clients know when there's more.

Add Octa as an MCP server to any compatible client (Claude Desktop,
Claude Code, MCP Inspector) pointing the `command` at the `octa`
binary with `--mcp` as the argument.
"#;

const SAVING: &str = r#"# Saving

- **File > Save** writes back to the original file (preserves format and
  settings).
- **File > Save As** lets you save to a new file, optionally in a different
  format.
- Closing a tab or quitting with unsaved changes prompts a confirmation
  dialog (**Save / Don't Save / Cancel**).
- For SQLite / DuckDB sources, saves are diff-based: only changed rows are
  updated, deleted rows are DELETEd, new rows are INSERTed. Schema changes
  (rename / add / drop column) are rejected; do those in another tool.
"#;

const SETTINGS_REFERENCE: &str = r#"# Settings Reference

Open **Help > Settings** (default **F3**). Categories are collapsible:

- **Appearance**: font size and family, theme, icon variant, custom font
  path, custom title bar.
- **Table View**: row numbers, alternating row colors, negative-number
  highlight, edit highlight, default mark color, line breaks, binary
  display mode (Binary / Hex / Text).
- **Search & Editor**: default search mode, tab size.
- **File-Specific**: column coloring for raw CSV/TSV, "warn before
  un-aligning" guard, "warn on date format change" banner, "read-only
  mode notice" toggle, notebook output layout.
- **SQL**: panel position, default row limit, autocomplete, editor font
  (JetBrains Mono / Match UI / System Monospace).
- **MCP**: default row limit (with **Unlimited** toggle) and per-cell
  byte cap for the `octa --mcp` server. Read at server startup, so
  changes require a restart.
- **Map**: default mode (Tiles / Geometry only), tile URL template,
  fall-back-to-geometry toggle for offline / blocked tile fetches.
- **Directory Tree**: sidebar position (left / right).
- **Shortcuts**: rebind any keyboard shortcut. Conflicting bindings are
  flagged.
- **Performance**: initial-load row cap (streaming readers), syntax-
  highlight size cap (raw editor fallback), and a user-extensible list
  of file extensions to open as plain text.
- **Files**: how many recent files to remember.
- **Window**: default size, start maximized.

Settings persist to:

- Linux: `~/.config/octa/settings.toml`
- macOS: `~/Library/Application Support/Octa/settings.toml`
- Windows: `%APPDATA%\Octa\settings.toml`
"#;

const SHORTCUTS_INTRO: &str = r#"# Shortcuts

Every action below can be rebound under **Help > Settings > Shortcuts**.
Unbound actions show `(none)`. The bindings shown are the current ones:
"#;
