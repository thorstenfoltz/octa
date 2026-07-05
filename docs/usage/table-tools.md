# Table Tools

A few quick utilities for reshaping and tidying the active table.

## Transpose

**Analyse > Transpose** swaps rows and columns into a new tab. The original
column names become the first column, and each original row becomes a column.
Everything is shown as text, since a transposed mix of column types has no single
type.

Transpose is limited to tables of at most 1000 rows, because each row becomes a
column and a very wide result would be unusable. Above that limit the status bar
tells you instead of running.

## Random sample

**Analyse > Random sample...** opens a small dialog where you type a row count
(100 by default). It then opens a new tab holding that many rows, chosen at random
from the active table. This is handy for eyeballing a fair cross-section of a big
file without scrolling all of it. If you ask for more rows than the table has, you
get them all.

## Tidy up

**Data > Tidy up...** cleans the current table in a single undoable step. Two
options:

- **Trim spaces from cells and headers** (on by default): removes leading and
  trailing spaces from text cells and column titles. Spaces inside a value are
  left alone.
- **Tidy column names to snake_case** (off by default): lowercases column names
  and replaces runs of punctuation or spaces with a single underscore.

A single **Undo** reverts the whole thing. Octa can already do this automatically
when opening a file; this lets you run it at any time.

## Clickable links

When a cell holds a web address (`http://` or `https://`), Octa shows it as an
underlined link. **Ctrl+click** opens it in your browser; a plain click still
selects the cell as usual.

Because Ctrl+click also toggles a cell in a multi-cell selection, on a link cell
the Ctrl+click opens the link instead of toggling that one cell. You can turn the
whole feature off with **Settings > Table View > Clickable web links**.
