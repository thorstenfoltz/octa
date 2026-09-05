# Export to PDF

**File → Export to PDF...** prints what the active tab is showing to a
paginated PDF: the grid, the [Summary](summary.md) tab, the
[Data Quality Report](data-quality-report.md) and its section tabs, a
comparison, or any other result tab. They are all tables, so they all export
the same way. The same entry sits on a tab's right-click menu, which exports
that tab.

## What ends up on the page

Exactly what you can see, and nothing you cannot:

- The **rows the current filter leaves**, in the **sort order** on screen.
- The **visible columns**, in their current order. Hidden columns stay hidden.
- **Colour marks** and **conditional formatting** colours, in the same palette
  the grid paints.
- Unsaved **cell edits**, because the export reads the cells the same way the
  grid does.

Values print as they are stored, without the thousands separators the grid can
add, the same as every other export.

## Pagination

Nothing is truncated to make the table fit. A long table pages **down**, a wide
one pages **across**, and the pages come out in reading order: all the columns
of the first band of rows, then the next band.

- The **header row repeats** on every page.
- **Frozen columns** repeat on every page across, so a page of columns 40 to 48
  still tells you which record you are looking at. Freeze them in the table view
  first (right-click a column header → **Freeze columns up to here**).
- A **footer** on every page carries the file name, the page number and the row
  range, plus the column range when the table needed more than one page across.
- A cell too long for its column is cut with an `...`; the column width is
  measured from the first 200 visible rows.

## The dialog

- **Page size**: A4 or Letter.
- **Orientation**: portrait or landscape. Landscape fits more columns on a page,
  portrait more rows.
- **Describe the view on the first page**: adds a line under the title naming
  the active filter and the row and column counts, so a printed page says what
  it is a page of. On by default.

The dialog tells you how many pages the export will be **before** you write it.
There is no row or column cap, so a five-million-row table really will produce
tens of thousands of pages: filter first, and let the page count tell you.

## What the document says

The title is the file name, or the tab label for a result tab. The document
chrome (the footer, the description line) is English, like the
[HTML report](report.md), because these are files that travel outside the app.

## Not the same thing as the HTML report

The [Report](report.md) is a separate document with charts and per-column
sections, written as HTML. If you want that as a PDF, open it in your browser
and print to PDF: the browser lays out its charts and tables properly. The PDF
export here prints the table you are looking at.
