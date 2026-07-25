# Release notes

This release is about getting many files into one table with less friction, and
about the interface holding up on a small screen. You can now union every table
in a folder, local or in the cloud, in one action; pick files by dragging a
selection box over them; reach a toolbar that is wider than the window; and read
a Markdown preview that actually looks like a rendered document.

## Union a whole folder

**Union every table in a cloud folder.** Right-click a folder in the cloud
sidebar and pick **Union tables in this folder...**, or **Union tables in this
folder and subfolders...** for a recursive sweep. Octa lists the folder, keeps
the objects it can read, downloads them, and opens the usual reconciliation
dialog, so a prefix full of `part-*.parquet` becomes one table without picking
the parts by hand. Columns are reconciled as always, so the files need not match
exactly. Very large folders stop at 500 files and the status bar says how many
were skipped.

**Reading many files no longer freezes the window.** Union used to read every
file on the interface thread, so a folder of forty JSON files locked the window
until the dialog appeared. The reading now happens in the background: the window
stays live and the status bar shows a spinner with a running count. For cloud
folders it reports both stages, first `Downloading files to union: 12/40`, then
the reading count, so a slow bucket never looks like a hang.

**The result keeps its format.** When every source shares one format, the
combined table remembers it, and **Save As** opens pre-filled with a matching
name, so forty JSON files in means one JSON file out in a single click. A mixed
selection has no single answer, so the picker opens with no suggestion and you
choose. Nothing is written until you save. One limit worth knowing: Octa
reconciles the columns of each source, so nested JSON comes back flattened, with
one column per leaf (`address.city` rather than a nested object).

Every detached result tab benefits from the same change: Summary, Pivot, Join and
the rest now suggest a sensible file name when you save them.

## Pick files by dragging over them

**Rubber-band selection in the file sidebar.** Press on empty space in the
directory tree and drag: a translucent band appears and every file it crosses is
selected, which is the quickest way to grab a run of files for **Union...**.
Hold **Ctrl** while dragging to add to what is already selected instead of
replacing it. Ctrl-click and Shift-click still work as before, and a plain click
still just opens the file.

**The same in the cloud sidebar.** The cloud tree gets an identical band, so
selecting twenty objects in a bucket is one drag rather than twenty Ctrl-clicks.

## A toolbar that fits a small screen

**The toolbar scrolls sideways.** On a laptop screen, or in a language with long
menu labels, the toolbar holds more than fits, and the right-hand end used to be
simply cut off with no way to reach it. Point at the toolbar and use the mouse
wheel, or drag the slim scrollbar under the row, and the rest comes into view.
The scrollbar sits in its own strip below the buttons rather than across them.

**The window buttons stay put.** With a custom title bar, minimise, maximise and
close keep their reserved place at the right edge and can no longer be pushed
off the window, however narrow it gets or however long the menu labels are.

## Markdown preview

The preview was cramped and hard to read in dark themes. It now reads like a
document:

- **Air between blocks and lines.** Paragraphs, headings, lists, quotes and code
  blocks get a proper vertical rhythm, and body text gets generous line spacing
  instead of lines packed against each other. Level-one and level-two headings
  are followed by a rule.
- **Code you can actually read.** Inline code and fenced blocks now take their
  background from the active theme's palette, so every dark theme gets a surface
  that stands out from the page instead of the near-invisible grey they all
  shared. Inline code is padded out from the text into a readable chip.
- **Wrapped list items line up.** A bullet point that runs onto a second line now
  continues under the first character of the text instead of jumping back under
  the bullet.
- **Lists with blank lines between items keep their bullets.** So do their
  indents, and a second paragraph inside one item is indented without sprouting a
  second bullet. A parent item's own text is no longer swallowed by its nested
  list.
- **Block quotes look like quotes again.** A quote is now a tinted block at
  whatever height it needs, rather than plain text with a single stripe glyph
  beside its first line.
- **Table headers are distinguishable** from the striped rows behind them.
- **The preview follows your text size.** The **Font size** setting and
  Ctrl+Plus / Ctrl+Minus zoom now scale the rendered document, which previously
  stayed at a fixed size whatever you chose.

The in-app documentation and the EPUB reader share this renderer, so both improve
with it.

## Fixes

**Jumping to a search match in a JSON or YAML tree lands on the match.** Deep
matches used to scroll to the wrong place, drifting further the further down the
document the match was, and the expand arrow could be clipped.

**Whitespace trim on load no longer touches text documents.** With **Trim
whitespace on load** enabled, Markdown, plain text, Jupyter notebooks and EPUBs
were being stripped like tabular data, which broke Markdown hard line breaks and
raised a spurious warning banner. Those formats are now left alone; trimming
still applies to real tables.

**No more stray "Untitled" tab.** Unioning files while sitting on Octa's empty
startup tab, or listing a bucket's contents as a table, no longer leaves a blank
"Untitled" beside the result. Those results reuse the blank tab you are already
on, the same way opening a file does.

**Clearer status bar hint.** The navigation field reads **Go to R:C or column
name** instead of abbreviating the word.
