//! In-app documentation window rendered with CommonMark. The shortcut table
//! is generated from the current user bindings so it never drifts from the
//! active behavior.

use eframe::egui;

use octa::ui;

use super::super::state::OctaApp;

/// Build the Markdown shortcut table rendered in the Documentation dialog.
fn build_shortcut_doc_table(shortcuts: &ui::shortcuts::Shortcuts) -> String {
    use strum::IntoEnumIterator;
    let mut s = String::from("| Shortcut | Action |\n|----------|--------|\n");
    for action in ui::shortcuts::ShortcutAction::iter() {
        let combo = shortcuts.combo(action);
        s.push_str(&format!("| {} | {} |\n", combo.label(), action.label()));
    }
    s
}

pub(crate) fn render_documentation_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.show_documentation_dialog {
        return;
    }
    let mut open = app.show_documentation_dialog;
    egui::Window::new("Documentation")
        .open(&mut open)
        .resizable(true)
        .collapsible(true)
        .default_size([800.0, 600.0])
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let shortcut_table = build_shortcut_doc_table(&app.settings.shortcuts);
                let docs = format!(r#"# Octa Documentation

## Getting Started

Open a file using **File > Open** (or **Ctrl+O**), or pass a file path as a command-line argument:

```
octa path/to/file.parquet
```

**Supported formats:** Parquet, CSV, TSV, JSON, JSONL, Excel (.xlsx), Avro, Arrow IPC, ORC, HDF5, XML, TOML, YAML, PDF, Markdown, Plain Text.

All formats support both reading and writing. When saving, the original format and settings (e.g. CSV delimiter) are preserved.

## Navigation

- **Arrow keys** move the selected cell up, down, left, or right
- **Scroll wheel** scrolls the table vertically
- **Shift + Scroll wheel** scrolls horizontally
- Click a **row number** to select the entire row (Ctrl+click to add to selection, Shift+click for range)
- Click a **column header** to select the entire column
- **Ctrl+A** selects all rows

## Editing

- **Double-click** a cell to start editing — the current text is selected so you can type to replace it, or click to position the cursor
- Click outside the cell or press **Tab** to confirm the edit
- **Escape** cancels the current edit
- **Ctrl+Z** to undo, **Ctrl+Y** to redo — works for cell edits, row/column operations, and color marks
- **Edit > Insert Row** adds a new empty row below the selected cell
- **Edit > Insert Column** opens a dialog to add a column (choose name and type)
- **Edit > Delete Row / Delete Column** removes the selected row or column
- **Edit > Move Row Up/Down** and **Move Column Left/Right** reorder data
- **Edit > Discard All Edits** reverts all unsaved changes
- **Drag a column header** to reorder columns via drag-and-drop
- **Double-click a column header** to rename it inline
- **Right-click a column header** to change the column data type

## Formulas

Cells support simple Excel-like formulas starting with **=**. Supported features:

- **Cell references**: A1, B2, AA1, etc. (column letter + row number, 1-based — column letters are shown in each column header)
- **Operators**: `+`, `-`, `*`, `/`
- **Parentheses**: `(A1 + B1) * 2`
- **Numeric literals**: `=A1 * 1.5`

Examples: `=A1+B1`, `=C1*2`, `=(A1+B1)/C1`

When inserting a new column via **Edit > Insert Column**, you can specify a formula in the **Formula** field. The formula acts as a template using row 1 references — it is automatically applied to every row (e.g. `=A1+B1` fills row 3 with `=A3+B3`).

Division by zero returns no result (the cell stays empty).

## Search & Filter

Use the search box in the toolbar to filter rows in real-time. Only rows containing a match are displayed.

Three search modes are available (selectable via the dropdown next to the search box):

- **Plain**: case-insensitive substring match
- **Wildcard**: `*` matches any sequence of characters, `?` matches one character
- **Regex**: full regular expression syntax

Use **Ctrl+F** to focus the search box from anywhere.

## Find & Replace

Open the replace bar with **Ctrl+H** or via **Search > Find & Replace**.

Type a search term and a replacement value, then:

- **Next** replaces the first matching cell value found in the table
- **All** replaces every matching cell value across all visible rows

Press **Escape** to close the replace bar.

## Color Marking

Right-click a **cell**, **row number**, or **column header** to open the context menu, then use the **Mark** submenu. Available colors: Red, Orange, Yellow, Green, Blue, Purple.

Mark precedence: cell marks take priority over row marks, which take priority over column marks.

To clear a mark, right-click and select **Clear Mark** from the context menu.

## View Modes

Switch between views using the **View** menu:

- **Table View** (default): structured tabular display with sorting, filtering, and editing
- **Raw Text**: shows the raw file content as plain text (available for text-based formats)
- **PDF View**: rendered page view (available for PDF files)
- **Markdown View**: rendered markdown (available for .md files)

## Tabs and Folder Sidebar

Every opened file is shown as a tab, even when only one file is open. Hovering a tab reveals the full file path — handy when several tabs share a file name.

**File > Open Directory…** opens a folder browser docked as a sidebar (left by default — switch to the right via **Settings > Directory Tree**). Clicking any file in the tree opens it in a new tab. **File > Close Directory** hides the sidebar without touching the open tabs.

## SQL Autocomplete and Case Conversion

In the SQL editor:

- As you type, a strip of suggestion chips appears below the editor with matching column names and SQL keywords. Click a chip to complete the current token. Toggle this off under **Settings > SQL > Autocomplete** (on by default).
- The **UPPER** / **lower** buttons (and the right-click context menu) convert the current selection, or the whole query when nothing is selected.

The same upper / lower case context menu is available in the Raw Text editor.

## Column Insertion Autofill

When typing a name in **Insert Column**, matching existing column names are shown as clickable chips — click to fill the name field.

## Saving

- **File > Save** writes changes back to the original file (preserves format and settings)
- **File > Save As** lets you save to a new file, optionally in a different format
- If you have unsaved changes and try to open a new file or close the application, a confirmation dialog appears

## Settings

Open **Help > Settings** to configure:

- **Font size**: adjusts text size across the entire application including table content
- **Default theme**: Light or Dark mode
- **Icon color**: choose from 12 color variants for the application icon
- **Default search mode**: which search mode is active by default (Plain, Wildcard, or Regex)
- **Show row numbers**: toggle the row number column on the left
- **Alternating row colors**: toggle zebra-stripe row backgrounds for easier reading
- **Negative numbers in red**: display negative numeric values in red
- **Highlight edited cells**: show a yellow background on cells that have been modified (off by default)

## Keyboard Shortcuts

The table below reflects your **current** bindings (customize them under
**Help > Settings > Shortcuts**). Unbound actions show `(none)`.

### Customizable shortcuts

{shortcut_table}
"#);
                egui_commonmark::CommonMarkViewer::new()
                    .show(ui, &mut app.tabs[app.active_tab].commonmark_cache, &docs);
            });
        });
    app.show_documentation_dialog = open;
}
