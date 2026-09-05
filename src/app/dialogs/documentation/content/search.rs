//! Finding things: search and replace, plain-language filtering,
//! multi-search, column filters, bookmarks and flagged-cell navigation.
//!
//! One of six topic files split out of `content.rs`, which held all 77 section
//! bodies in a single 4,261-line, 192 KB file. Text moved verbatim; the parent
//! `content/mod.rs` re-exports every constant, so `documentation::sections()`
//! is untouched.
//!
//! ASCII only: egui's bundled font renders typographic punctuation as tofu.

pub const SEARCH: &str = r#"# Search & Replace

The toolbar search box matches rows in real time. Three modes (selectable in
the dropdown next to the box):

- **Plain**: case-insensitive substring.
- **Wildcard**: `*` matches any sequence, `?` matches one character.
- **Regex**: full regular expression syntax.

## Case, whole word, and scope

Three controls beside the search box refine matching:

- **Aa** toggles **case-sensitive** matching. Off (the default) matches
  regardless of capitalisation in every mode.
- **W** toggles **whole-word** matching, so `cat` matches the word "cat"
  but not "category" or "scatter".
- The **scope** dropdown limits the search to a single column or, by
  default, **All columns**. The dropdown always shows the current scope so
  you can see at a glance whether you are searching one column or the whole
  table.

These apply to the table filter and to the in-place highlight.

## Search history

Recent search queries are remembered across sessions. When there is
history, a **Recent** dropdown appears beside the search box; pick an
entry to re-run it. How many queries are kept is set by **Search history
size** under **Settings -> Search & Editor** (default 5; set it to 0 to
turn the history off). The list is stored in `search_history.json` in
Octa's config directory.

## Filter or highlight

A toggle button beside the search box switches how matches are shown:

- **Filter** (the default): non-matching rows are hidden, as before.
- **Highlight**: every row stays visible and the matching cells are
  highlighted in place.

The default is set in **Settings -> Search result display**. The table view
honours the toggle. Text and tree views (Jupyter notebooks, the JSON and YAML
trees, Markdown and the raw text editor) always highlight, because hiding free
text or collapsing tree nodes makes no sense there.

When matches are highlighted, the search bar shows a **count** (current / total)
and two buttons to step through matches. **Enter** jumps to the next match and
**Shift+Enter** to the previous one while the search box is focused; the view
scrolls the current match into view.

**Ctrl+F** focuses the search box from anywhere; **Ctrl+H** opens the
**Find & Replace** bar above the table:

- **Next** replaces the first match found.
- **All** replaces every match across visible rows.

**Escape** closes the replace bar.
"#;

pub const ASK_FILTER: &str = r#"# Ask: Filtering in Plain Language

The search bar has an **Ask** toggle. With it on, what you type is not
matched against the table: it is sent to a configured assistant, which
turns it into filters.

Turning Ask on retargets the search box: it empties, takes focus, and its
placeholder changes to "Ask a question, then press Enter". While Ask is
on, typing does not filter as you go, so a half-written question never
empties the table; nothing happens until you press Enter.

Type something like "revenue over 1000 in Germany" and press Enter. The
categorical part lands in the ordinary column filters, and comparisons
appear as removable chips above the table:

    From your question:   revenue greater than 1000  x     Clear all

Which assistant answers is always visible: the dropdown beside the toggle
names the profile that will run, and you can pick another. With no
profile configured, both controls are greyed out and the tooltip says to
set one up in Settings > Chat / Assistant first.

Nothing is hidden. Every condition lands as an ordinary, editable filter,
so a wrong interpretation can be corrected by hand rather than being a
mystery.

It is one request with no tools and no follow-up, so typing in the search
box can never turn into an autonomous session. If the reply cannot be
understood, or names a column that does not exist, nothing is applied and
the status bar says why.

Only filters and an optional sort come back. Adding columns or changing
data is the chat panel's job, not this one.
"#;

pub const MULTI_SEARCH: &str = r#"# Multi-search

The toolbar **Search** field filters the active tab. **Multi-search**
covers the other half of the problem: find the same string across
**every open tab** or **every file in a directory** at once.

Open via **Search > Multi-search** or **F6** (remappable). A docked
panel slides up at the bottom of the window with its own query box,
mode picker, and scope selector.

## Scopes

- **All Open Tabs**: walk every loaded tab. Runs synchronously, no
  background thread -- cheap even with several tabs open.
- **Directory**: walk every readable file in a folder (top level only,
  not recursive). Runs in a background thread; results stream into the
  panel as files finish parsing. Use the **Pick directory...** button
  to choose the folder.

## Modes

Plain / Wildcard / Regex -- same semantics as the main search bar.
Invalid regexes surface a one-line error above the result list.

## Jumping to results

Each result row reads:

    <source>  row N  <column name>  <snippet>

Clicking jumps to that cell. Directory-scope hits that aren't already
open get loaded into a fresh tab first.

## Limits

- **Per-file size cap** (Settings > Performance > Multi-search file
  cap, default 50 MB). Oversized files end up in the skipped chip
  (see below) with their actual size. Tick **Unlimited** beside the
  setting to scan every file whatever its size.
- **Cap of 10,000 hits per scan**, 1,000 per file -- a runaway regex
  on a huge dataset can't pin the UI.
- **In-memory rows only**. For lazy formats (Parquet, CSV/TSV) the
  scan covers whatever's currently loaded; rows still streaming in
  the background aren't searched until they land.

## Skipped files

When a reader fails on an individual file (binary blob, malformed
text, encoding mismatch, ...) Octa moves on to the next file and
collects the failing one in a **N file(s) skipped -- click to
expand** chip above the result list. The expanded view shows each
file's name plus the reason (size cap or parser error); the full
path is visible on hover. The list resets on the next search.
A failure in one file does not hide results from files that
searched fine.

Press **Cancel** to stop a running directory scan at the next file
boundary. Whatever hits were already collected stay in the panel.
"#;

pub const COLUMN_FILTER: &str = r#"# Column Filter

Excel-style per-column value-set filter. Pick a column, see its unique
values as checkboxes, uncheck the ones to hide.

## Opening the dialog

- **Search > Column Filter...** in the toolbar.
- The default shortcut (remappable; check Settings > Shortcuts for the
  current binding) opens the same dialog.
- **Right-click any column header > Filter values...** opens the dialog
  pre-seeded on that column.
- The status-bar **Filter** chip (visible when any column has an active
  filter) opens the dialog on the first filtered column.

## Using the dialog

- The top combo picks the column being filtered. Switching columns
  commits the in-progress checks to the previous column automatically,
  so multiple filters can be edited in one session.
- **Find** narrows the value list when a column has many unique values.
  Up to 5000 values are shown at a time; if more match, a hint tells you
  to narrow further with the search box.
- **Select all** and **Select none** operate on the currently visible
  (post-search) subset, not the whole list.
- **Apply** commits the draft. "All checked" and "none checked" are
  both interpreted as "no filter active" for that column.
- **Clear filter on this column** removes the column's filter entirely.
- **Cancel** discards the in-progress draft.

## Behaviour

- Column filters AND with each other: a row must satisfy every active
  column filter to remain visible.
- Column filters also AND with the toolbar text search.
- A small accent-coloured dot appears next to filtered column headers so
  active filters are visible at a glance.
- Filters live with the tab. Closing the tab discards them; they are
  not saved to disk.
- "Select none + Apply" hides every row in the current view, just like
  unchecking every checkbox by hand. Use "Clear filter on this column"
  to remove the filter entirely.

## Saving filtered data

**File > Save As** writes only the **currently visible** rows when a
filter (text search or column filter) is active. The on-disk file is a
snapshot of the view; the in-memory table is left untouched so you can
keep working on the full dataset.

Regular **File > Save** always writes the **full table** back to the
source path. The visible filter does not change what Save writes; this
keeps the source file safe from accidental data loss while filters are
active.
"#;

pub const PROBLEM_NAV: &str = r#"# Jump to Flagged Cells

Validation violations and detected outliers are painted in the grid, which
is no help in a table with two hundred thousand rows.

- **F10** jumps to the next flagged cell.
- **Shift+F10** jumps to the previous one.

Both wrap around, and the status bar reports `Problem 3 of 27` as you go.
The two sets are treated as one list of "cells worth looking at", ordered
top to bottom then left to right.

Rows hidden by the current search or column filter are skipped, so the
counter always matches what is actually on screen. If nothing is flagged,
the status bar says so rather than moving the selection.

Both keys are remappable under Settings > Shortcuts (Navigation).
"#;

pub const FILTER_TO_MARKED: &str = r#"# Filter to Marked

**Edit > Filter to marked** hides everything except what you have colour-marked,
so you can drill down to the rows and columns you care about. Choose the same
menu entry again (now labelled "Clear filter to marked") to restore the full
view.

## What stays

- **Marked rows** stay; unmarked rows are hidden.
- **Marked columns** stay; unmarked columns are hidden.
- **Marked cells** are handled per a setting (**Settings > Filter to marked:
  cells**): keep the cell's row (the default), keep its column, or keep both.

Filter to marked combines with the search box and column filters, exactly like
every other filter - they all apply together. Turning it off restores any
columns you had hidden manually beforehand.
"#;

pub const BOOKMARKS: &str = r#"# Bookmarks

Bookmarks are named jump points inside a table, handy for returning to the same
spots in a large file.

## Using bookmarks

- Select a cell (or a row), then add a bookmark to name it. You can do this from
  the toolbar **Bookmarks** dropdown (**Add bookmark...**), from **Data > Add
  bookmark...**, by right-clicking a cell and choosing **Add bookmark...**, or
  with the Ctrl+Alt+B shortcut.
- Pick a bookmark from the toolbar **Bookmarks** dropdown to jump straight to it.
- Use the small **x** next to a bookmark in that dropdown to delete it.

Bookmarks are session-only and fixed-position: they live while the tab is open
and point at a row/column position, so they do not follow later row inserts or
deletes.
"#;
