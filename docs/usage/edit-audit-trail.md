# Edit Audit Trail

A docked panel listing every pending (unsaved) cell edit in the active
table: the row and column, the value before, and the value after.

Open it via **View > Edit audit trail**. The shortcut is unbound by
default (every clipboard-safe Ctrl+Shift letter is already taken); bind
one under **Settings > Shortcuts** if you use it often.

The trail lists pending cell edits only. Structural changes, such as
rows or columns that were added or deleted, are not listed: they follow
a separate undo path, not the cell-edit overlay this panel reads. Once
a table is saved, its pending edits are gone (they are now just the
file), so the trail for that tab goes back to empty.

## Controls

- **Jump** - selects that cell in the table and scrolls it into view.
  If the row is currently hidden by an active filter, the cell is still
  selected, but the view is left alone, since there is nowhere on
  screen to scroll it to.
- **Revert** - undoes just that one edit, restoring its original value,
  without touching any other pending edit. This goes through the same
  edit path as typing into a cell, so it lands on the undo stack: Ctrl+Z
  after a revert brings the edit back, the same as undoing any other
  change. Disabled in read-only mode, with a tooltip explaining why;
  viewing the trail is never gated, only reverting is.
- **Copy as SQL** - copies every pending edit to the clipboard as
  `UPDATE` statements, one per edited row. Only the columns you actually
  changed appear in the `SET` clause, and the `WHERE` clause is built
  from the row's key using its **original** value, so a row whose key
  cell you edited is still found where the server still has it. Copying
  changes no data, so the button works in read-only mode as well. It is
  disabled only when there is nothing pending to copy.

## What the copied SQL targets

For a tab opened from a live database connection, the statements use
that connection's dialect (quoting, identifier escaping and date
literals), its schema and its table name, and the key columns the
connection reported. They are ready to paste into your own SQL session.

For a tab opened from a file, there is no server and no key. The
statements are then named after the source file's stem and match rows on
the first column's original value instead. The panel says so in a note
above the button: treat that output as a starting point to edit, not a
script to run as-is. A database tab whose engine or schema offered no
key falls back to exactly the same template, with the same note.

## Where the panel docks

**Settings > Table > Edit audit trail position** picks which edge of
the window the panel docks to (left, right, top or bottom). It is
resizable within the window like the SQL panel, the Assistant panel and
the Column Navigator.
