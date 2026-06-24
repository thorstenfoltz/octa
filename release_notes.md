# Release notes

This release is about polish. The raw and Markdown views are nicer to read, the
Summary statistics are more accurate, and editing cells and column headers now
behaves the way you expect.

## Viewing files

**Colour in the raw view for JSON, YAML, XML and TOML.** These structured formats
are now syntax-highlighted in the Raw Text view, the same as source code, so a
config or data file reads clearly instead of as flat monospace. Large files still
fall back to plain text under the existing syntax-highlight size limit.

**Markdown opens in Preview, and links work.** Markdown files now open as the
rendered document by default, rather than the side-by-side editor. A link in the
preview opens in your system browser when you click it. The Preview / Split / Edit
toggle in the toolbar still switches modes whenever you want to write or edit.

## More accurate statistics

**Empty cells count as missing.** The Summary tab's null count now includes empty
text cells, not just true nulls. A column of blank text used to report zero
missing values; it now reports them correctly.

**Exact values, no rounding.** Summary statistics are shown at full precision.
Figures that were previously rounded to six decimals, such as sums, ranges, the
interquartile range, and the distinct ratio, now show their real computed value.

## Editing tables

**Click into a cell you are editing.** When a cell is in edit mode with its text
selected, clicking inside the text now moves the cursor to that point instead of
leaving edit mode. You no longer have to reach for the arrow keys.

**Renaming a column works like editing a cell.** Double-clicking a column header
selects the whole name, ready to be replaced, and you can click into it to place
the cursor exactly where you want, the same as editing any cell.
