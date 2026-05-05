//! In-app documentation. Categorized into sections so the dialog can offer a
//! sidebar nav (left) + content pane (right), mirroring the structure of the
//! Settings dialog. The shortcut table is generated from the user's current
//! bindings each time the dialog opens, so it never drifts from behavior.

use eframe::egui;

use octa::ui;
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::OctaApp;

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
        ("Color Marking", MARKING.to_string()),
        ("View Modes", VIEW_MODES.to_string()),
        ("Tabs & Folder Sidebar", TABS.to_string()),
        ("SQL View", SQL_VIEW.to_string()),
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
        DialogSize::Maximized => window.fixed_rect(ctx.screen_rect().shrink(8.0)),
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
                        egui_commonmark::CommonMarkViewer::new().show(
                            ui,
                            &mut app.tabs[app.active_tab].commonmark_cache,
                            body,
                        );
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

## Read + write formats

- Tabular columnar / data-science: Parquet, Avro, Arrow IPC, ORC
- Plain text / interchange: CSV, TSV, JSON, JSONL, XML, TOML, YAML
- Office: Excel (`.xlsx`)
- Databases (diff-on-save row edits, no schema changes): SQLite (`.sqlite`,
  `.sqlite3`, `.db`), DuckDB (`.duckdb`, `.ddb`), GeoPackage (`.gpkg`)
- Statistical: SPSS (`.sav`, `.zsav`), Stata (`.dta`)
- Other: dBase / DBF, Jupyter notebooks (`.ipynb`), PDF, Markdown (`.md`),
  Plain Text

## Read-only formats

- SAS (`.sas7bdat`)
- R Datasets (`.rds`, `.rdata`, `.rda`)
- HDF5 (`.h5`, `.hdf5`, `.hdf`)

When saving, the original format and settings (e.g. CSV delimiter) are
preserved. Database writes only update changed rows and reject schema
changes — rename or add columns in another tool first.
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

- **Double-click** a cell to start editing — the current text is selected so
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

The toolbar search box filters rows in real time — only rows containing a
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

const MARKING: &str = r#"# Color Marking

Right-click a **cell**, **row number**, or **column header** to open the
context menu, then use the **Mark** submenu. Available colors: Red, Orange,
Yellow, Green, Blue, Purple.

The **Edit > Mark** menu — and the **Mark** keyboard shortcut (default
**Ctrl+M**) — apply a single color to the **whole current selection**: a row
block, column block, multi-cell selection, or single cell. The shortcut uses
the color set under **Settings > Table > Default mark color** (Yellow by
default).

Mark precedence: cell > row > column. To clear a mark, right-click and choose
**Clear Mark**.
"#;

const VIEW_MODES: &str = r#"# View Modes

Switch via the **View** menu — only modes applicable to the current file are
enabled.

- **Table View** (default): structured tabular display with sorting,
  filtering, and editing.
- **Raw Text**: shows the file content as plain text. For CSV/TSV the toolbar
  exposes Quote / Escape / Delimiter combos and an **Align Columns** toggle
  with per-column coloring.
- **PDF View**: rendered page bitmaps plus selectable per-page text. Editing
  text in the table view updates the text frame under each page; bitmaps
  refresh on Save.
- **Markdown View**: rendered markdown for `.md` files.
- **Notebook View**: rendered Jupyter notebook with cell outputs.
- **JSON Tree**: collapsible tree view for JSON / JSONL.

The **Cycle view mode** shortcut (default **F4**, remappable) advances through
the modes available for the current tab.
"#;

const TABS: &str = r#"# Tabs & Folder Sidebar

Every opened file has a tab — even when only one is open. Hovering a tab
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

Each query opens a fresh connection — there is no persistent SQL state
between runs.
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
  (rename / add / drop column) are rejected — do those in another tool.
"#;

const SETTINGS_REFERENCE: &str = r#"# Settings Reference

Open **Help > Settings** (default **F3**). Categories are collapsible:

- **Appearance**: font size and family, theme, icon variant, custom font path.
- **Table View**: row numbers, alternating row colors, negative-number
  highlight, edit highlight, default mark color, line breaks, binary display
  mode (Binary / Hex / Text).
- **Search & Editor**: default search mode, tab size.
- **File-Specific**: column coloring for raw CSV/TSV, "warn before
  un-aligning" guard, "warn on date format change" banner, notebook output
  layout.
- **SQL**: panel position, default row limit, autocomplete.
- **Directory Tree**: sidebar position (left / right).
- **Shortcuts**: rebind any keyboard shortcut. Conflicting bindings are
  flagged.
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
