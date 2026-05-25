# Keyboard Shortcuts

Every action below is remappable under **Settings → Shortcuts**.
The bindings shown are the defaults shipped with Octa.

The Settings dialog flags conflicting bindings. If you rebind one
action to a combo already used by another, both rows highlight and
the **Apply** button explains the conflict. Resolve it (rebind one
of them, or clear the binding on one) before applying.

## File operations

| Action                 | Default                                       | Notes                                                                                             |
|------------------------|-----------------------------------------------|---------------------------------------------------------------------------------------------------|
| New file               | <kbd>Ctrl</kbd>+<kbd>N</kbd>                  | Open an empty scratch tab.                                                                        |
| Open file              | <kbd>Ctrl</kbd>+<kbd>O</kbd>                  | File picker (multi-select supported).                                                             |
| Save file              | <kbd>Ctrl</kbd>+<kbd>S</kbd>                  | Write back to the original path.                                                                  |
| Save file as…          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | New path + optional new format.                                                                   |
| Export schema…         | <kbd>F7</kbd>                                 | Open the Schema Export dialog with all 7 targets. See [Schema Export](../usage/schema-export.md). |
| Reload file from disk  | <kbd>Ctrl</kbd>+<kbd>R</kbd>                  | Discards unsaved changes after a confirmation.                                                    |
| Close current tab      | <kbd>Ctrl</kbd>+<kbd>W</kbd>                  | Prompts when there are unsaved changes.                                                           |
| Reopen last closed tab | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Walks back through the last 10 closed tabs.                                                       |
| Quit application       | <kbd>Ctrl</kbd>+<kbd>Q</kbd>                  | Prompts when any tab has unsaved changes.                                                         |

## Tabs

| Action       | Default                                         | Notes           |
|--------------|-------------------------------------------------|-----------------|
| Next tab     | <kbd>Ctrl</kbd>+<kbd>Tab</kbd>                  | Wraps to first. |
| Previous tab | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Wraps to last.  |

## Search

| Action                | Default                                       | Notes                                                                                                                 |
|-----------------------|-----------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| Focus search box      | <kbd>Ctrl</kbd>+<kbd>F</kbd>                  | Filter the table in real time.                                                                                        |
| Toggle find & replace | <kbd>Ctrl</kbd>+<kbd>H</kbd>                  | Replace bar above the table.                                                                                          |
| Open column filter    | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Per-column value filter. See [Column Filter](../usage/search-and-filter.md#column-filter).                            |
| Find duplicate rows…  | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> | Dedupe-key picker + Highlight / New-tab output. See [Editing → Find duplicates](../usage/editing.md#find-duplicates). |
| Multi-search panel    | <kbd>F6</kbd>                                 | Cross-tab + directory grep with a docked result list. See [Multi-search](../usage/search-and-filter.md#multi-search). |
| Open Chart tab        | <kbd>F5</kbd>                                 | Open a new tab dedicated to plotting the active table. Same as **Analyse → Chart**. See [Chart](../usage/chart.md).   |

## Navigation in the table

| Action                       | Default                                       | Notes                                                 |
|------------------------------|-----------------------------------------------|-------------------------------------------------------|
| Go to cell (focus nav input) | <kbd>Ctrl</kbd>+<kbd>G</kbd>                  | Status-bar field accepts `R5:C3`, row #, column name. |
| Jump to first row            | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> |                                                       |
| Jump to last row             | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↓</kbd> |                                                       |
| Jump to first column         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>←</kbd> |                                                       |
| Jump to last column          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>→</kbd> |                                                       |

## Selection

| Action                        | Default                      | Notes                                                             |
|-------------------------------|------------------------------|-------------------------------------------------------------------|
| Select all rows               | <kbd>Ctrl</kbd>+<kbd>A</kbd> | Inactive when a text editor is focused (lets Ctrl+A select text). |
| Extend row selection up       | <kbd>Ctrl</kbd>+<kbd>↑</kbd> |                                                                   |
| Extend row selection down     | <kbd>Ctrl</kbd>+<kbd>↓</kbd> |                                                                   |
| Extend column selection left  | <kbd>Ctrl</kbd>+<kbd>←</kbd> |                                                                   |
| Extend column selection right | <kbd>Ctrl</kbd>+<kbd>→</kbd> |                                                                   |

## Editing

| Action                    | Default                                           | Notes                                         |
|---------------------------|---------------------------------------------------|-----------------------------------------------|
| Edit current cell         | <kbd>F2</kbd>                                     | Same as double-clicking.                      |
| Insert row below          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> | New empty row.                                |
| Duplicate selected row(s) | <kbd>Ctrl</kbd>+<kbd>D</kbd>                      | Copies the selected row(s) immediately below. |
| Delete selected row(s)    | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd>     |                                               |
| Undo last change          | <kbd>Ctrl</kbd>+<kbd>Z</kbd>                      | Covers cell edits, structural changes, marks. |
| Redo last undone change   | <kbd>Ctrl</kbd>+<kbd>Y</kbd>                      |                                               |

## Clipboard

| Action         | Default                      | Notes                          |
|----------------|------------------------------|--------------------------------|
| Copy selection | <kbd>Ctrl</kbd>+<kbd>C</kbd> | TSV format on the clipboard.   |
| Cut selection  | <kbd>Ctrl</kbd>+<kbd>X</kbd> | Copies, then clears the cells. |
| Paste          | <kbd>Ctrl</kbd>+<kbd>V</kbd> | Splits on tabs + newlines.     |

## Marking

| Action                          | Default                      | Notes                                                 |
|---------------------------------|------------------------------|-------------------------------------------------------|
| Mark selection (default colour) | <kbd>Ctrl</kbd>+<kbd>M</kbd> | Colour is **Settings → Table → Default mark colour**. |

## Text-case

| Action                   | Default                                     | Notes                                                       |
|--------------------------|---------------------------------------------|-------------------------------------------------------------|
| Uppercase selected cells | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>U</kbd> | Also works on TextEdit selections (SQL editor, raw editor). |
| Lowercase selected cells | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>L</kbd> |                                                             |

## Zoom

| Action     | Default                      | Notes                             |
|------------|------------------------------|-----------------------------------|
| Zoom in    | <kbd>Ctrl</kbd>+<kbd>+</kbd> | 5% increments; 25% to 500% range. |
| Zoom out   | <kbd>Ctrl</kbd>+<kbd>-</kbd> |                                   |
| Reset zoom | <kbd>Ctrl</kbd>+<kbd>0</kbd> | Back to 100%.                     |

## View

| Action                | Default                                       | Notes                                                                               |
|-----------------------|-----------------------------------------------|-------------------------------------------------------------------------------------|
| Cycle view mode       | <kbd>F4</kbd>                                 | Walks Table → Raw → Markdown → … skipping modes not applicable to the current file. |
| Toggle read-only mode | <kbd>F8</kbd>                                 | Session-only; not persisted.                                                        |
| Toggle SQL panel      | <kbd>Ctrl</kbd>+<kbd>J</kbd>                  | Same as **Analyse → SQL**. See [SQL panel](../usage/sql.md).                        |
| Auto-fit all columns  | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Same algorithm as double-clicking a column-header seam, applied to every column.    |
| Compare selected tabs | <kbd>F9</kbd>                                 | Requires exactly one tab to be Ctrl-clicked in the multi-selection set.             |

## SQL panel

| Action            | Default                                       | Notes                                                            |
|-------------------|-----------------------------------------------|------------------------------------------------------------------|
| Export SQL result | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Save the current SQL result to a file. No-op when no result yet. |

## Inspector / dialogs

| Action                      | Default                                       | Notes                                                                                                          |
|-----------------------------|-----------------------------------------------|----------------------------------------------------------------------------------------------------------------|
| Open documentation          | <kbd>F1</kbd>                                 | This documentation, in-app.                                                                                    |
| Open settings               | <kbd>F3</kbd>                                 |                                                                                                                |
| Open column inspector       | <kbd>Ctrl</kbd>+<kbd>I</kbd>                  | Column-level metadata + type info for the active tab.                                                          |
| Show column value frequency | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> | Top-N values + counts for the column of the selected cell. See [Value Frequency](../usage/value-frequency.md). |

## Cheat-sheet (most-used)

If you only remember a handful:

|                                               |                   |
|-----------------------------------------------|-------------------|
| <kbd>Ctrl</kbd>+<kbd>O</kbd>                  | Open              |
| <kbd>Ctrl</kbd>+<kbd>S</kbd>                  | Save              |
| <kbd>Ctrl</kbd>+<kbd>F</kbd>                  | Search            |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Y</kbd>   | Undo / Redo       |
| <kbd>F4</kbd>                                 | Cycle view        |
| <kbd>F8</kbd>                                 | Read-only         |
| <kbd>Ctrl</kbd>+<kbd>J</kbd>                  | SQL panel         |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Reopen closed tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Fit all columns   |

## See also

- [Settings → Shortcuts](settings.md#shortcuts), the rebinding UI.
- [Table View](../usage/table-view.md) for context on the navigation
  and selection shortcuts.
- [Editing](../usage/editing.md) for context on the editing shortcuts.
