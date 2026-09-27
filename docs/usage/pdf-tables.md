# Tables from PDFs

Invoices, bank statements and reports often arrive as PDFs. Open one in
Octa like any other file and it finds the tables inside.

## Opening

**File -> Open**, the folder sidebar, or `octa invoice.pdf`. When the
PDF holds one table it opens straight away. With several, the table
picker lists them as `Page 1, table 1`, `Page 2, table 1` and so on,
with their columns and row counts; pick one and it opens as a normal
tab. From there it is an ordinary table: filter it, fix it, save it as
xlsx or csv.

The first row of a table becomes the column names. Numbers and dates are
recognised the same way as in a CSV.

## What counts as a table

A PDF has no tables inside, only pieces of text placed on the page.
Octa groups the text into lines, splits each line where there is a wide
gap, and treats a run of lines that line up into **two or more
columns** as a table. Titles, headings and paragraphs above or between
tables are left out.

That covers most machine-made PDFs. It gets it wrong when:

- a header cell spans two columns: those two columns merge into one;
- a cell wraps onto a second line: the table ends there;
- the columns are close together with no visible gap between them.

## Scanned pages

A scanned page is a **picture** of a page. There is no text in it, only
pixels, and Octa does no OCR (text recognition). So a table on a
scanned page cannot be read.

Octa tells you instead of showing nothing:

- A PDF that is **only** scanned pages: *This PDF is made of scanned
  images with no text layer, so its tables are pictures. Octa reads
  tables stored as text, not pictures. Run the PDF through OCR first,
  then open the result.*
- A PDF where **some** pages are scanned: the tables from the other
  pages open as usual, and a note above the table names the scanned
  pages that were not read.

Most scanners and PDF tools can add a text layer ("OCR", "make
searchable"). Open that version in Octa.

## Other messages

- **No table found**: the PDF has text, but none of it lines up into
  columns. A letter or a report written in paragraphs gives this.
- **Password-protected**: Octa cannot open protected PDFs. Save an
  unprotected copy first. PDFs that are only protected against editing
  or printing open normally.

## From the command line and the assistant

Every command that reads a file reads PDFs too. Commands that take a
single table (`--convert`, `--head`, ...) read the first one; `--describe`
lists them all and `--table` picks one of several there:

```bash
octa --convert invoice.pdf invoice.csv
octa --describe invoice.pdf --table "Page 1, table 2"
```

To save a later table, open the PDF in the GUI, pick it in the table
picker and use **File -> Save as**.

The MCP tools (`read_table`, `list_tables` and the rest) work the same
way.
