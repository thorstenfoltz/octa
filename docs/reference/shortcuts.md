# Keyboard Shortcuts

Every action below is remappable under **Settings → Shortcuts**.
The bindings shown are the defaults shipped with Octa.

Click **Record** on a row and press the combination you want. While Octa waits
for that press the keys do nothing else, so recording <kbd>Ctrl</kbd>+<kbd>S</kbd>
records it rather than saving the file. <kbd>Esc</kbd> stops recording.

Two actions can never share a combination. If the one you press is already
taken, Octa names the action holding it and offers **Take it over**: the key
moves to the action you are recording, and the previous owner is left unbound
(it shows `(none)` and you can record a new key for it). Nothing is written
until you click **Apply**, so **Cancel** still discards the whole lot.

## File operations

| Action                        | Default                                       | Notes                                                                                                                                                |
|-------------------------------|-----------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|
| New file                      | <kbd>Ctrl</kbd>+<kbd>N</kbd>                  | Open an empty scratch tab.                                                                                                                           |
| Open file                     | <kbd>Ctrl</kbd>+<kbd>O</kbd>                  | File picker (multi-select supported).                                                                                                                |
| Save file                     | <kbd>Ctrl</kbd>+<kbd>S</kbd>                  | Write back to the original path.                                                                                                                     |
| Save file as…                 | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | New path + optional new format.                                                                                                                      |
| Save database changes as SQL… | *(unbound)*                                   | Render a database tab's pending edits as a reviewable script instead of applying them. See [Database Connections](../usage/database-connections.md). |
| Save to database…             | *(unbound)*                                   | Write the open table into a live connection or a DuckDB / SQLite file. See [Database Connections](../usage/database-connections.md).                 |
| Export workbook…              | *(unbound)*                                   | Write several open tabs into one `.xlsx`, one sheet per tab. See [Saving](../usage/saving.md).                                                       |
| Open URL…                     | *(unbound)*                                   | Open an `http(s)://` or cloud address as a file. See [Cloud Storage](../usage/cloud-storage.md).                                                     |
| Export schema…                | <kbd>F7</kbd>                                 | Open the Schema Export dialog with all ten targets. See [Schema Export](../usage/schema-export.md).                                                  |
| Reload file from disk         | <kbd>Ctrl</kbd>+<kbd>R</kbd>                  | Discards unsaved changes after a confirmation.                                                                                                       |
| Close current tab             | <kbd>Ctrl</kbd>+<kbd>W</kbd>                  | Prompts when there are unsaved changes.                                                                                                              |
| Reopen last closed tab        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Walks back through the last 10 closed tabs.                                                                                                          |
| Quit application              | <kbd>Ctrl</kbd>+<kbd>Q</kbd>                  | Prompts when any tab has unsaved changes.                                                                                                            |
| Open table folder             | *(unbound)*                                   | Open a Delta / Iceberg / dataset directory as one table. See [Supported Formats](../getting-started/supported-formats.md).                           |

## Tabs

| Action       | Default                                         | Notes                                                                |
|--------------|-------------------------------------------------|----------------------------------------------------------------------|
| Next tab     | <kbd>Ctrl</kbd>+<kbd>Tab</kbd>                  | Wraps to first.                                                      |
| Previous tab | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Wraps to last.                                                       |
| Rename tab…  | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>T</kbd>     | Display name only; the file path is unchanged. Also tab right-click. |

## Search

| Action                                     | Default                                       | Notes                                                                                                                 |
|--------------------------------------------|-----------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| Focus search box                           | <kbd>Ctrl</kbd>+<kbd>F</kbd>                  | Filter the table in real time.                                                                                        |
| Toggle find & replace                      | <kbd>Ctrl</kbd>+<kbd>H</kbd>                  | Replace bar above the table.                                                                                          |
| Open column filter                         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Per-column value filter. See [Column Filter](../usage/search-and-filter.md#column-filter).                            |
| Find duplicate rows…                       | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> | Dedupe-key picker + Highlight / New-tab output. See [Editing → Find duplicates](../usage/editing.md#find-duplicates). |
| Find near-duplicates…                      | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd> | Fuzzy duplicate clusters (typos, spacing, word order). See [Find Near-Duplicates](../usage/find-near-duplicates.md).  |
| Open multi-search panel                    | <kbd>F6</kbd>                                 | Cross-tab + directory grep with a docked result list. See [Multi-search](../usage/search-and-filter.md#multi-search). |
| Run inventory on expanded cloud connection | *(unbound)*                                   | Lists the objects under the expanded cloud prefix as a table. See [Cloud Inventory](../usage/cloud-inventory.md).     |

## Navigation in the table

| Action                        | Default                                       | Notes                                                      |
|-------------------------------|-----------------------------------------------|------------------------------------------------------------|
| Go to cell (focus nav input)  | <kbd>Ctrl</kbd>+<kbd>G</kbd>                  | Status-bar field accepts `R5:C3`, row #, column name.      |
| Jump to first row             | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> |                                                            |
| Jump to last row              | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↓</kbd> |                                                            |
| Jump to first column          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>←</kbd> |                                                            |
| Jump to last column           | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>→</kbd> |                                                            |
| Scroll up one page            | <kbd>Ctrl</kbd>+<kbd>PgUp</kbd>               | Advances selection by one visible page; spreadsheet-style. |
| Scroll down one page          | <kbd>Ctrl</kbd>+<kbd>PgDn</kbd>               | Mirror of **Scroll up one page** in the other direction.   |
| Add bookmark...               | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>B</kbd>   | Names the current selection as a session bookmark.         |
| Jump to next flagged cell     | <kbd>F10</kbd>                                | Steps through validation violations and detected outliers. |
| Jump to previous flagged cell | <kbd>Shift</kbd>+<kbd>F10</kbd>               | Same set, other direction. Both wrap and respect filters.  |

## Selection

| Action                        | Default                      | Notes                                                             |
|-------------------------------|------------------------------|-------------------------------------------------------------------|
| Select all rows               | <kbd>Ctrl</kbd>+<kbd>A</kbd> | Inactive when a text editor is focused (lets Ctrl+A select text). |
| Extend row selection up       | <kbd>Ctrl</kbd>+<kbd>↑</kbd> |                                                                   |
| Extend row selection down     | <kbd>Ctrl</kbd>+<kbd>↓</kbd> |                                                                   |
| Extend column selection left  | <kbd>Ctrl</kbd>+<kbd>←</kbd> |                                                                   |
| Extend column selection right | <kbd>Ctrl</kbd>+<kbd>→</kbd> |                                                                   |

**Ctrl+click a cell** toggles it in a disjoint multi-cell selection.
Mark / Copy / Cut then operate on every selected cell, following the
same precedence Ctrl+M uses from the keyboard.

## Editing

| Action                    | Default                                           | Notes                                         |
|---------------------------|---------------------------------------------------|-----------------------------------------------|
| Edit current cell         | <kbd>F2</kbd>                                     | Same as double-clicking.                      |
| Insert row below          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> | New empty row.                                |
| Duplicate selected row(s) | <kbd>Ctrl</kbd>+<kbd>D</kbd>                      | Copies the selected row(s) immediately below. |
| Delete selected row(s)    | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd>     |                                               |
| Number format...          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>     | Per-column number formatting dialog.          |
| Undo last change          | <kbd>Ctrl</kbd>+<kbd>Z</kbd>                      | Covers cell edits, structural changes, marks. |
| Redo last undone change   | <kbd>Ctrl</kbd>+<kbd>Y</kbd>                      |                                               |

## Clipboard

| Action                 | Default                                       | Notes                                             |
|------------------------|-----------------------------------------------|---------------------------------------------------|
| Copy selection         | <kbd>Ctrl</kbd>+<kbd>C</kbd>                  | TSV format on the clipboard.                      |
| Cut selection          | <kbd>Ctrl</kbd>+<kbd>X</kbd>                  | Copies, then clears the cells.                    |
| Paste                  | <kbd>Ctrl</kbd>+<kbd>V</kbd>                  | Splits on tabs + newlines.                        |
| Copy as Markdown table | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | GitHub-flavoured Markdown table of the selection. |

## Marking

| Action                          | Default                                     | Notes                                                 |
|---------------------------------|---------------------------------------------|-------------------------------------------------------|
| Mark selection (default colour) | <kbd>Ctrl</kbd>+<kbd>M</kbd>                | Colour is **Settings → Table → Default mark colour**. |
| Filter to marked                | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>M</kbd> | Keep only marked rows/columns; press again to clear.  |

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

| Action                      | Default                                       | Notes                                                                                                                  |
|-----------------------------|-----------------------------------------------|------------------------------------------------------------------------------------------------------------------------|
| Cycle view mode             | <kbd>F4</kbd>                                 | Walks Table → Raw → Markdown → … skipping modes not applicable to the current file.                                    |
| Toggle read-only mode       | <kbd>F8</kbd>                                 | Session-only; not persisted.                                                                                           |
| Toggle SQL panel            | <kbd>Ctrl</kbd>+<kbd>J</kbd>                  | Same as **Analyse → SQL**. See [SQL panel](../usage/sql.md).                                                           |
| Open chart tab              | <kbd>F5</kbd>                                 | Open a new tab dedicated to plotting the active table. Same as **Analyse → Chart...**. See [Chart](../usage/chart.md). |
| Toggle chat assistant panel | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>A</kbd> | Same as **Analyse → Assistant**. See [Chat Assistant](../usage/chatbot.md).                                            |
| Auto-fit all columns        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Same algorithm as double-clicking a column-header seam, applied to every column.                                       |
| Compare selected tabs       | <kbd>F9</kbd>                                 | Requires exactly one tab to be Ctrl-clicked in the multi-selection set.                                                |

## SQL panel

| Action            | Default                                       | Notes                                                                                                                                             |
|-------------------|-----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------|
| Export SQL result | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> | Save the current SQL result to a file. No-op when no result yet.                                                                                  |
| Run SQL on server | *(unbound)*                                   | Runs the editor statement on the tab's database connection instead of local DuckDB. See [Database Connections](../usage/database-connections.md). |

## Dialogs

| Action                             | Default                                       | Notes                                                                                                          |
|------------------------------------|-----------------------------------------------|----------------------------------------------------------------------------------------------------------------|
| Open documentation                 | <kbd>F1</kbd>                                 | This documentation, in-app.                                                                                    |
| Open settings                      | <kbd>F3</kbd>                                 |                                                                                                                |
| Show column value frequency        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> | Top-N values + counts for the column of the selected cell. See [Value Frequency](../usage/value-frequency.md). |
| Pivot / Unpivot...                 | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> | See [Pivot / Unpivot](../usage/pivot.md).                                                                      |
| Transform column...                | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> | See [Transform column](../usage/transform-column.md).                                                          |
| Conditional formatting...          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd> | See [Conditional formatting](../usage/conditional-formatting.md).                                              |
| Conditional column...              | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>J</kbd> | If / else-if / else CASE column. See [Transform column](../usage/transform-column.md).                         |
| Anonymise columns...               | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Y</kbd> | Mask / scramble sensitive columns. See [Anonymise Columns](../usage/anonymize-columns.md).                     |
| Data validation...                 | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>G</kbd> | See [Data validation](../usage/data-validation.md).                                                            |
| Sort by columns...                 | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> | Multi-column sort.                                                                                             |
| Summary tab                        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> | See [Summary](../usage/summary.md).                                                                            |
| Data quality report...             | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>Q</kbd>   | See [Data Quality Report](../usage/data-quality-report.md).                                                    |
| Rename columns…                    | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>R</kbd>   | Bulk column rename. See [Rename Columns](../usage/rename-columns.md).                                          |
| Fill missing values...             | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>I</kbd>   | See [Fill Missing Values](../usage/fill-missing-values.md).                                                    |
| Union tables...                    | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>N</kbd>   | Needs two open tabs. See [Union Tables](../usage/union-tables.md).                                             |
| Detect outliers...                 | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>O</kbd>   | See [Detect Outliers](../usage/detect-outliers.md).                                                            |
| Detect PII...                      | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>P</kbd>   | See [Detect PII](../usage/detect-pii.md).                                                                      |
| Clean-up suggestions               | *(unbound)*                                   | Toggles the panel; opening it scans. See [Clean-up Suggestions](../usage/cleanup-suggestions.md).              |
| File internals...                  | *(unbound)*                                   | Physical layout of the open file. See [File Internals](../usage/file-internals.md).                            |
| Compare with database or cloud...  | *(unbound)*                                   | See [Compare with Database or Cloud](../usage/compare-with-database.md).                                       |
| Join key finder...                 | *(unbound)*                                   | See [Join Key Finder](../usage/join-key-finder.md).                                                            |
| Join diagnostics...                | *(unbound)*                                   | See [Join Diagnostics](../usage/join-diagnostics.md).                                                          |
| Harmonise schemas...               | *(unbound)*                                   | See [Harmonise Schemas](../usage/harmonise-schemas.md).                                                        |
| Toggle Ask (plain-language filter) | *(unbound)*                                   | Retargets the search box at a question instead of a filter.                                                    |
| Drop duplicate rows...             | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>H</kbd> | See [Drop Duplicate Rows](../usage/drop-duplicate-rows.md).                                                    |
| Join tables                        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Q</kbd> | Needs two open tabs. See [Join Tables](../usage/join-tables.md).                                               |
| Partition by column                | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Writes one file per group. See [Partition by Column](../usage/partition-by-column.md).                         |
| Time series...                     | *(unbound)*                                   | Time buckets and rolling windows. See [Time Series](../usage/time-series.md).                                  |
| Batch convert...                   | *(unbound)*                                   | See [Batch Convert](../usage/batch-convert.md).                                                                |
| Schema drift...                    | *(unbound)*                                   | See [Schema Drift](../usage/schema-drift.md).                                                                  |
| Report...                          | *(unbound)*                                   | HTML profiling report. See [Report](../usage/report.md).                                                       |
| Fuzzy join...                      | *(unbound)*                                   | See [Fuzzy Join](../usage/fuzzy-join.md).                                                                      |
| Data drift...                      | *(unbound)*                                   | How one dataset changed between two versions. See [Data Drift](../usage/data-drift.md).                        |
| Relationship map...                | *(unbound)*                                   | Which tables link to which, and on which columns. See [Relationship Map](../usage/relationship-map.md).        |

All of these are rebindable in **Settings → Shortcuts**, which refuses to let two
actions share the same combination.

An action listed as *(unbound)* ships with no default combination, because every
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+letter is already taken (and
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>X</kbd> / <kbd>V</kbd> fire
the clipboard on the table regardless). Assign one yourself if you use it often;
the menu entry works either way.

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
