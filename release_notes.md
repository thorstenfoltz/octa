# Release notes

This release adds a batch of productivity features across saving, reshaping,
tidying, and inspecting data, plus friendlier tabs and a set of new keyboard
shortcuts. Octa can now save your work automatically, transpose or sample a
table in a click, tidy up a table on demand, open web links from cells, bookmark
spots in a large file, score a table's data quality, rename many columns at once,
and give tabs your own names.

## Saving

**Auto-save.** Octa can now save your open files automatically on a timer.
Turn it on under **Settings > Files** and set an interval in minutes (for
example 5, to save every five minutes). It is off by default. Each interval it
writes every open tab that has unsaved changes and already exists as a file on
disk, and shows a brief "Auto-saved N files" note when it does. It never
interrupts you: tabs never saved to disk, cloud tabs while cloud writing is off,
saves that would normally ask a question (a rounding format, or a database schema
change), and a tab you are editing at that moment are all skipped quietly.

## Reshaping and tidying

**Transpose.** **Analyse > Transpose** swaps a table's rows and columns into a
new tab: the original column names become the first column, and each original row
becomes a column. Limited to tables of at most 1000 rows.

**Tidy up.** **Data > Tidy up...** cleans the current table in one undoable step:
trim stray spaces from cells and column titles, and optionally convert the column
names to `snake_case`. Octa could already do this when opening a file; now you
can run it at any time.

**Rename many columns at once.** **Columns > Rename columns...** opens a box
pre-filled with every column of the active table, one per line. Add a comma and a
new name to rename a column, and leave a line unchanged to keep it. A live preview
shows what will be renamed, warns about names that clash, and the whole batch
reverts with a single Undo.

## Looking at your data

**Data quality report.** **Analyse > Data quality report...** opens a scored,
per-column report (missing values, distinct ratio, outliers, likely personal
data, type consistency, with a 0-100 score per column) and shows the overall
score in the status bar.

**Random sample.** **Analyse > Random sample...** opens a new tab with a number
of rows you choose, picked at random from the active table, for eyeballing a fair
cross-section of a big file without scrolling all of it.

**Filter to marked.** **Edit > Filter to marked** hides everything except the
rows and columns you have marked, so you can zoom in on just the cells you care
about; run it again to clear. While it is active, the sequential row-number
column appears alongside the original numbers, as with any other filter.

**Clickable web links.** A cell that holds a web address (`http` or `https`) now
shows as an underlined link; **Ctrl+click** opens it in your browser, while a
plain click still selects the cell. Turn it off under **Settings > Table View**.

## Tabs

**Bookmarks.** Mark a spot in a table and jump back to it later. Add a bookmark
from the toolbar **Bookmarks** dropdown, from **Data > Add bookmark...**, by
right-clicking a cell, or with **Ctrl+Alt+B**; pick one from the dropdown to jump
straight to it. Bookmarks last for the session.

**Rename a tab.** Right-click a tab and choose **Rename tab...** (or press
**Ctrl+Alt+T**) to give it your own label. This changes only what the tab shows;
the file path and the name on disk are unchanged, and hovering the tab still
reveals the full path. Clear the name to go back to the file name.

## Keyboard shortcuts

New shortcuts, all in the **Ctrl+Alt** family so nothing existing is replaced:
Filter to marked (**Ctrl+Alt+M**), Data quality report (**Ctrl+Alt+Q**), Rename
columns (**Ctrl+Alt+R**), Add bookmark (**Ctrl+Alt+B**), Rename tab
(**Ctrl+Alt+T**), plus default bindings for Fill missing values
(**Ctrl+Alt+I**), Union tables (**Ctrl+Alt+N**), Detect outliers
(**Ctrl+Alt+O**), and Detect PII (**Ctrl+Alt+P**). All remain remappable in
Settings.

## Translations

The new menu items, dialogs, and settings are available in all 32 supported
languages (with English text as the fallback for the newest strings).
