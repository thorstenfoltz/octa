# Editing

Octa is primarily a reader, but it has a real editor underneath:
inline cell editing, row and column structural changes, colour marks,
and full undo/redo for everything.

This page covers structural operations. For navigation and selection
within the Table view, see [Table View](table-view.md). For cell
formulas (`=A1+B1`), see [Formulas](formulas.md).

## Editing cells

Double-click any cell to start editing. The current text is selected
so typing replaces; click to position the cursor. **Tab** / **Enter**
confirm; **Escape** cancels.

Values are parsed based on the column's declared type:

| Column type          | Accepts                                                | Notes                                                                                      |
|----------------------|--------------------------------------------------------|--------------------------------------------------------------------------------------------|
| `Int64`, `Int32`, …  | Integer digits, with optional `-`                      | Trailing whitespace stripped. Non-numeric → null.                                          |
| `Float64`, `Float32` | Decimals, scientific notation                          | `1e10` parses; `nan`/`inf` parse to those values.                                          |
| `Boolean`            | `true`/`false` (case-insensitive), `1`/`0`, `yes`/`no` |                                                                                            |
| `Utf8`, `LargeUtf8`  | Any text                                               |                                                                                            |
| `Date32`             | ISO `YYYY-MM-DD` plus common dialects                  | See [Date Inference](../reference/date-inference.md)                                       |
| `Timestamp`          | ISO 8601 with time                                     |                                                                                            |
| `Binary`             | Per the active display mode                            | See [**Settings → Table View → Binary display mode**](../reference/settings.md#table-view) |

Edits are tracked in an **overlay**, so the underlying rows aren't
mutated until **File → Save** or **Edit → Discard All Edits**. This
means undo can walk back through every edit and structural change
even after dozens of mutations.

## Inserting rows

- **Edit → Insert Row** (or right-click → Insert Row) adds a new
  empty row below the selected cell.
- The default shortcut is configurable under
  [**Settings → Shortcuts → InsertRowBelow**](../reference/settings.md#shortcuts).
- New rows get null values for every column. Edit individual cells
  to populate them.

For database-backed tabs (SQLite, DuckDB, GeoPackage), new rows are
flagged as INSERTs on save, so they'll be added to the underlying
table with auto-generated IDs where applicable (see
[Saving → Databases](saving.md#database-files-sqlite-duckdb-geopackage)).

## Inserting columns

**Edit → Insert Column…** opens a dialog with three fields:

- **Name**: the new column's name. Must be unique.
- **Type**: pick from a dropdown (Int64, Float64, Utf8, Boolean,
  Date32, Timestamp, Binary, …).
- **Formula**: optional. An Excel-like expression
  (e.g. `=A1*1.5`, `=B1+C1`). Treated as a row-1 template and
  applied per-row.
- **Insert at position**: 0-indexed; 0 puts the new column at the
  far left, blank appends to the end.

See [Formulas](formulas.md) for the expression syntax.

For database tables, schema changes (adding / renaming / removing
columns) are **rejected on save**. Make schema edits in another tool
first and reload.

## Deleting rows

- Select a row by clicking its row number.
- **Edit → Delete Row** (or right-click → Delete Row).
- Multi-select with Ctrl/Shift to delete several at once.

Deleted rows are remembered until save. For database-backed tabs,
they become DELETE statements in the diff-based save flow.

## Deleting columns

- Right-click a column header → **Delete column**.
- Or **Edit → Delete Columns…** opens a multi-select dialog for
  bulk operations.

For databases, see the schema-change note above.

## Moving rows and columns

- **Edit → Move Row Up / Down** moves the selected row.
- **Edit → Move Column Left / Right** moves the selected column.
- Or drag the row number / column header directly with the mouse.

Reordering does not change the underlying data, just the display
order. For file formats with a fixed column order on disk (Parquet,
Arrow), Save persists the new order.

## Undo / Redo

[**Ctrl+Z**](../reference/shortcuts.md#editing) undoes;
[**Ctrl+Y**](../reference/shortcuts.md#editing) redoes. Both
stacks are bounded only by available memory.

Tracked:

- Cell edits.
- Row / column insertions, deletions, moves, bulk reorderings.
- Column type changes (**Change Type** submenu).
- Colour marks (Cell / Row / Column).

Not tracked:

- Column renames are applied directly, with no undo entry.
- View-mode changes (Table → Raw → Markdown).
- Sort / filter changes.
- Window-state changes (zoom, splitter positions).

The Edit menu displays the current shortcut next to **Undo** and
**Redo**; entries are greyed-out when the respective stack is empty.

### Read-only mode

**F8** ([`ToggleReadOnly`](../reference/shortcuts.md#view)) toggles
a session-only read-only mode. Every editing path short-circuits:
double-click on a cell doesn't enter edit mode, the
[raw text editor](view-modes/raw-text.md) renders non-interactive,
and structural shortcuts no-op. **Undo/Redo are also disabled**
under read-only.

Useful for poking around files without risking an accidental edit.
The status bar shows `[Read-only]` while active.

## Find duplicates

**Search → Find duplicates…** (also <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>)
opens a modal that:

1. Lists every column with a checkbox. Tick the columns you want to
   use as the dedupe **key**. The Apply button is greyed until at
   least one column is selected.
2. Lets you pick what happens with the duplicates:
   - **Highlight rows in place (Orange mark)** — every row whose key
     matches another row gets an orange row mark in the active table.
     Use **Edit → Mark → Clear all marks** to remove them.
   - **Open duplicates in a new tab** — clones the columns and just
     the duplicate rows into a fresh scratch tab. The source tab is
     untouched. The new tab has no source path so a Save prompts
     for one.

Two rows are duplicates when **every** checked column has the same
displayed text. Hashing is text-based so it works across mixed types,
but `Int(1)` and `Float(1.0)` render as `"1"` vs `"1.0"` and therefore
do **not** dedupe — change the column type first if you need them to.

A row whose key only matches itself is **not** a duplicate. The
result always comes in pairs (or larger groups).

If no duplicates are found, the status bar reports that and nothing
else changes.

The dialog seeds its key from the currently selected column (or the
column of the selected cell), so the common one-column dedupe is two
keys away: <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> →
<kbd>Apply</kbd>.

## Parse in new tab

Reinterprets a piece of the active table as a fresh file in a chosen
format. Handy when a cell holds JSON you want to explore as a tree, a
row is really a structured record you want to view as YAML, or the
whole table should be re saved as Markdown without overwriting the
original.

The flow runs the chosen payload through a `tempfile::NamedTempFile`
and the standard file open code path, so the new tab uses the same
reader the format would normally get on disk. The new tab opens with
no `source_path`, so **Save** prompts for a new location instead of
silently writing back to `/tmp`.

### Triggers

- **Right-click a cell → Parse in new tab…** is the fastest entry
  point and is what most workflows use.
- **Edit → Parse in new tab…** in the toolbar opens the same dialog
  for the current selection.

The dialog has no shortcut by default; it is menu and context only.

### Scopes

The dialog header shows the scope picked up from the current selection.
A scope decides what becomes the payload:

| Scope           | What gets serialised                                                                                                              |
|-----------------|-----------------------------------------------------------------------------------------------------------------------------------|
| **Cell**        | A synthetic 1×1 table with the source column name as the header and the cell value as the only data row.                          |
| **Row**         | A synthetic single row table whose headers are the full source column names and whose values are the row's cells, all as strings. |
| **Column**      | A synthetic single column table with the source column name as the header and every cell of that column as the data rows.         |
| **Whole table** | The active table is serialised through the same format writer that **Save As** uses, so headers and types round trip exactly.     |

Synthetic cells are always typed as strings; let date inference or
manual type changes promote them after the parse if you want typed
columns.

### Formats

The dropdown lists the text style formats Octa can both produce and
re-read, in this order:

1. **JSON** (default, since the original motivation was unflattening
   JSON shaped cell payloads).
2. **JSON Lines**
3. **YAML**
4. **TOML**
5. **XML**
6. **CSV**
7. **TSV**
8. **Markdown**
9. **Plain Text**

Binary or schema strict formats (Parquet, Excel, HDF5, …) are
deliberately absent: rendering arbitrary cell content into them would
need binary bytes and would mostly produce noise.

**Plain Text** is the one mode that passes cells through verbatim with
no schema concept. The other modes go through a real serializer, so a
malformed Cell scope payload that does not parse as the chosen format
surfaces the parser error in the new tab's banner.

### CSV and TSV delimiter

Picking CSV or TSV reveals a one character **Delimiter** field
underneath the format dropdown. Defaults are `,` for CSV and a tab
character for TSV; switching between the two updates the field
automatically so the common case needs no thought.

### View mode on open

The new tab opens in the chosen format's default view mode:

- JSON / JSON Lines / YAML → JSON or YAML Tree.
- Markdown → the Markdown preview / split view.
- CSV / TSV / TOML / XML → Table view.
- Plain Text → Raw view.

Use <kbd>F4</kbd> to cycle into a different view mode if the default is
not what you want.

### When to reach for this versus the JSON tree

Use the **JSON Tree view** when the *whole file* is JSON or YAML and
you want to drill in or rename keys in place. Use **Parse in new
tab…** when JSON or YAML shaped text lives *inside* a cell, row, or
column of a tabular file and you want to lift it out into its own
tree without touching the source tab.

## Discard all edits

**Edit → Discard All Edits** reverts every change since the file was
opened (or last saved). It clears the edit overlay, the structural
change flag, and the undo / redo stacks.

A confirmation dialog protects against misfires.

## Right-click context menu

Right-click any table region for context-aware actions:

| Click target  | Actions available                                                                                                                                                       |
|---------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Cell          | Copy / Cut / Paste, Mark, Insert / Delete / Move row, Rename / Insert / Delete / Move column, Sort A–Z / Z–A, Parse in new tab                                          |
| Row number    | Copy / Cut / Paste row, Mark row, Insert / Delete / Move row                                                                                                            |
| Column header | Rename, Copy / Cut / Paste, Mark column, Sort A–Z / Z–A, Insert / Delete column, **Change Type** (String / Int64 / Float64 / Boolean / Date32 / Timestamp), Move column |

## See also

- [Formulas](formulas.md) covers the `=A1+B1` syntax and the Insert
  Column formula field.
- [Colour Marking](colour-marking.md) highlights cells, rows, and
  columns.
- [Saving](saving.md) writes back to disk, with format-specific
  notes for databases.
- [Search & Filter](search-and-filter.md) includes Find & Replace
  via **Ctrl+H**.
