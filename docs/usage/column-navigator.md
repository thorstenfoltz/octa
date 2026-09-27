# Column Navigator

The Column Navigator is a docked panel listing every column of the active
table, with a search box and per-column show/hide, freeze and reorder
controls. It is a second way in to the same show/hide, freeze and column-order
state the Columns menu and the header right-click menu already drive, useful
on a table with many columns where scrolling the header to find one is
slower than typing its name.

Open it via **View > Column navigator**. The shortcut is unbound by default
(every clipboard-safe Ctrl+Shift letter is already taken); bind one under
**Settings > Shortcuts** if you use it often.

## Controls

- **Search box** - type to narrow the list to columns whose name contains
  the text, case-insensitive. Clear it to see every column again.
- **Show all** / **Hide all** - show every hidden column at once, or hide
  every column except one. Hide all never removes the last visible column,
  so the table is never left with nothing to show.
- **Eye checkbox** - show or hide that one column. The same state the
  header's right-click "Hide column" and "Columns > Show hidden columns"
  read and write.
- **Freeze** - freeze that column and every column before it in the table,
  exactly like the header's "Freeze columns up to here". Click it again on
  the last frozen column to unfreeze everything.
- **Drag handle** - drag a row up or down to reorder that column in the
  table. Disabled in read-only mode, with a tooltip explaining why; showing,
  hiding and freezing stay available even then, since they only change how
  the table is displayed, not the data itself.

## Where the panel docks

**Settings > Table > Column navigator position** picks which edge of the
window the panel docks to (left, right, top or bottom). It is resizable
within the window like the SQL panel, the Assistant panel and Multi-search.

## Read-only mode

Show/hide and freeze are display-only and work in read-only mode. Reordering
columns is a structural change to the table, so it is blocked there, the
same rule every other structural edit follows.
