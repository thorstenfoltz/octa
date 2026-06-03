# Release notes

A round of bug fixes and small usability improvements: parsing now
respects multi-cell and multi-column selections, the Number format
dialog can round several columns at once, and a few dialogs that were
stuck at a fixed size are now resizable.

## Parse in new tab: multi-cell and multi-column

**Parse in new tab** now honours the full selection instead of a single
target.

- **Cell scope** with several marked cells (Ctrl+click) serialises the
  whole block: the bounding grid of the selected rows by columns, with
  any unselected cell in that block left blank. A single selected cell
  behaves as before.
- **Column scope** with several selected columns serialises all of them
  together as a multi-column table, rather than just the one column under
  the cursor.

Previously both cases only ever opened a single cell or column in the new
tab. The row-shaping is now unified, so Cell, Row, single-Column, and the
new multi-selection cases all flow through one path and keep their
headers.

## Number format: round several columns at once

The per-column **Number format** dialog (right-click a numeric header, or
**Edit -> Number format...**) gained an **Apply to columns** checklist.

- One decimals + rounding configuration can now be applied to several
  numeric columns in a single pass. Selecting multiple columns before
  opening the dialog pre-checks them; **All** / **None** toggle the whole
  list.
- Changes apply **live** to every checked column; unchecking a column
  drops its format. The list scrolls, and the dialog is resizable, so
  dragging it taller shows more columns at once.

## Resizable dialogs

Three dialogs that were pinned to a fixed size now resize freely in both
directions:

- **Date/Time calculation** (**Edit -> Date/Time calculation...**).
- **Number format**.
- The **rounding-on-save** prompt.

The fix was twofold: the windows were re-centred every frame (which
blocked the resize handles), and egui had persisted their old fixed size
under the title-derived key. They now use stable window ids and the same
body/footer layout the other resizable dialogs use. The Date/Time
calculation dialog's column chooser also opens a tall dropdown, so long
column lists no longer scroll three or four entries at a time.

## Documentation and README

- The **README** gained a Fixed-width (FWF) format row, a Source code /
  config row, a **Docker / Containers** section (pull, one-shot CLI run,
  stdio `--mcp`, Podman), and the full current MCP tool list (the stale
  "eleven tools" count is gone).

GitHub Pages (the mkdocs site) was updated to match:

- **Tips & Recipes** "Convert a messy CSV to a clean Parquet" recipe
  gained a **Via MCP** subsection alongside the GUI and CLI ones.
- The **Table view** Number format section documents the new
  multi-column **Apply to columns** checklist.
- The **Editing -> Parse in new tab** scope table documents the
  multi-cell and multi-column behaviour.
