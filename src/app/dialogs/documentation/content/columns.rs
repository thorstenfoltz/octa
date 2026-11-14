//! Working on columns: tools, transforms, anonymisation, marking,
//! conditional formatting, validation and the row-level clean-ups.
//!
//! One of six topic files split out of `content.rs`, which held all 77 section
//! bodies in a single 4,261-line, 192 KB file. Text moved verbatim; the parent
//! `content/mod.rs` re-exports every constant, so `documentation::sections()`
//! is untouched.
//!
//! ASCII only: egui's bundled font renders typographic punctuation as tofu.

pub const COLUMN_TOOLS: &str = r#"# Column Tools

## Hide and show columns

Right-click any column header and pick **Hide column** to remove it
from the view. Hidden columns are still part of the table on disk:
Save and Save as both write them out. Use **Columns > Show hidden
columns** to bring everything back at once. This is a per-tab,
session-only setting; closing the tab or reopening the file clears
the hidden set.

To hide several at once, select the columns (Ctrl+click or Shift+click
their headers) and pick **Columns > Hide selected columns**, or
right-click one of the selected headers. With no column selected, the
entry hides the columns of the selected cells. One column always stays
visible.

## Hide and show rows

Right-click a row number or a cell and pick **Hide row**. To hide
several, select the rows first (Ctrl+click or Shift+click their row
numbers) and pick **Hide selected rows** from the same menu or from
**Edit > Hide selected rows**. With no row selected, the Edit menu
entry hides the rows of the selected cells.

- The data stays in the table. Save writes every row. Save as asks
  whether to write the visible rows or all of them, as it does for a
  filter.
- A hidden row stays hidden when you sort, or insert and delete rows
  around it.
- The status bar shows **Hidden rows** with their count. Click it, or
  pick **Edit > Show hidden rows**, to bring them all back.

Hiding rows is per tab and session-only, and is not offered on a table
that is read page by page (a very large file).

## Copy column name(s)

Right-click any column header and pick **Copy column name(s)** to
copy the header text to the clipboard. If you have multiple columns
selected (Ctrl-click their headers) and right-click one of them, all
selected names are joined with newlines. Useful for building SQL
SELECT lists or scripts from Octa's view of the file.

## Freeze columns

Right-click a column header and pick **Freeze columns up to here** to
pin that column and every column to its left, exactly like freezing
panes in a spreadsheet. The pinned columns stay visible at the left
edge while the rest of the table scrolls horizontally underneath, so
an ID or name column never scrolls out of sight in a wide table. A
thin separator marks the boundary; **Unfreeze all columns** in the
same menu reverts.

The freeze is per tab and session-only, like column widths. If the
window gets too narrow to keep the whole frozen band and still scroll,
Octa temporarily pins fewer columns and restores the full band when
there is room again.
"#;

pub const COLUMN_NAVIGATOR: &str = r#"# Column Navigator

The Column Navigator is a docked panel listing every column of the active
table, with a search box and per-column show/hide, freeze and reorder
controls. Open it via **Columns > Column navigator**.

It is a second way in to the same show/hide, freeze and column-order state
the Columns menu and the header right-click menu already drive - toggling a
column here or there always agrees, since there is only one place any of it
is actually stored.

## Controls

- **Search box** - type to narrow the list to columns whose name contains
  the text, case-insensitive.
- **Show all** / **Hide all** - show every hidden column at once, or hide
  every column except one, so the table is never left with nothing to show.
- **Eye checkbox** - show or hide that one column, same as the header's
  right-click "Hide column".
- **Freeze** - freeze that column and every column before it, exactly like
  the header's "Freeze columns up to here". Click it again on the last
  frozen column to unfreeze everything.
- **Drag handle** - drag a row to reorder that column in the table.
  Disabled in read-only mode; showing, hiding and freezing stay available
  even then, since only reordering changes the table's structure.
- **Up / down arrows**: move that column one place, which is one place left
  or right in the table. Up is greyed out on the first column and down on
  the last. Disabled in read-only mode, like the drag handle.

**Settings > Table > Column navigator position** picks which edge of the
window the panel docks to.
"#;

pub const CHANGE_TYPE: &str = r#"# Change Column Type

Every reader guesses a column's type when it opens the file, and a
single stray value is enough to make it guess "text". **Change type**
fixes that after the fact, which is the whole point: load-time problems
get fixed once the file is open, never behind a gate before it.

## Two doors, one set of rules

- **Right-click a column header -> Change type** is the fast one. Pick a
  type and it converts on the spot, with no dialog to answer. If some
  values in the column would not convert, it opens the dialog below
  instead of half-converting a column you clicked through in one
  gesture.
- **Columns -> Change type...** opens that dialog directly, for when you
  want to look before you leap.

Both run exactly the same conversion, so they can never disagree.

## What happens to values that do not convert

They keep their original text. They are never blanked and the
conversion is never refused because of them: a column declared as a
number can still hold the one cell that reads `n/a`, and that is a far
more useful answer than either losing the value or being told "no".

Those cells are flagged as problem cells, so <kbd>F10</kbd> walks
through them the same way it walks validation failures and outliers.

## Dates

Dates go through the same seven layouts the loader itself uses, and the
layout is chosen for the whole column: whichever one reads the most
values wins, and every value is then read that way. So a German column
of `18.09.2026` converts and comes out as `2026-09-18`, instead of
being refused for not already looking like ISO. An ambiguous column
(`01/02/2020` reads as both DD/MM and MM/DD) is read as DD/MM.

The vote reads the first 10,000 values rather than the whole column,
because it parses each one under all seven layouts: seven parses per
value, in one go, on the thread that draws the window. Uncapped on a
five-million-row column that is thirty-five million parses and a
multi-second freeze on a single click. It never changes a reported
count, since every loaded row is classified under whichever layout
wins.

**Settings > Performance > Change type > Date layout sample** raises or
removes that limit. It is only worth raising when a column's first
values are not representative of the rest, such as a file sorted so
every ISO-formatted row comes first.

## Every date column at once

**Columns -> Detect dates** finds the text columns that hold nothing but
dates and turns all of them into date columns in one go. It is the same
check **Settings > Opening files > Detect dates on load** runs when a
file opens, and the way to get dates with that setting off, which it is
by default.

It runs in the background, so the window keeps responding on a table of
millions of rows. Only columns whose first thousand values look like
dates are read in full. While it runs the tab is read-only and neither
sorts nor loads more rows, because the answer goes back in by row
number. One <kbd>Ctrl+Z</kbd> undoes every column it converted.

A column that two layouts fit (`01/02/2020`) gets the same question the
loader asks. A column where most but not all values are dates stays
text, with a note naming the values that are not; **Change type**
converts it anyway and keeps those values as text.

## Strict or mixed

By default the conversion is **mixed**: what parses is converted, what
does not keeps its text. That is what makes it useful on a real column,
where one `n/a` should not cost you the other 999 numbers.

Sometimes that is the wrong outcome: a column of "mostly numbers" sorts
and sums as a number column while quietly leaving the stragglers out.
Tick **Only convert if every value converts** and the conversion is
refused whenever anything would fail, changing nothing at all - not the
values, not the type, nothing on the undo stack. The dialog says how
many values blocked it so you can fix them or untick the box.

## The types

Text, Whole number, Decimal number, Boolean, Date, Date and time.
Converting to **Text** can never fail, so it is always available as a
way out.

The whole conversion is one entry on the undo stack: a single
<kbd>Ctrl</kbd>+<kbd>Z</kbd> puts the column back exactly as it was,
including the values that kept their text.

On a partly loaded file the counts describe the rows that are loaded,
and the dialog says so.
"#;

pub const TRANSFORMS: &str = r#"# Transform Column

Transform Column reshapes your data with a single click, the way you would
clean up a messy spreadsheet by hand. Open it via
**Data > Transform column...**. Pick an operation, fill in its options, and
press **Apply**. Each transform is undoable (Ctrl+Z), session-only until you
save, and respects read-only mode.

## Operations

- **Split column** - break one column into several. Split on a **delimiter**
  (for example a comma, so `a,b,c` becomes three cells), a **regular
  expression**, or a **fixed width** (every N characters). New columns are
  named after the source with a `_1`, `_2`, ... suffix; rows with fewer parts
  get empty cells.
- **Merge columns** - join two or more columns into one new column with a
  separator you choose (like joining First and Last name with a space).
- **Fill down** / **Fill up** - copy the nearest non-empty value into the
  empty cells above or below it. Handy for un-merging the "only show the
  group name on the first row" style of export.
- **Extract pattern** - pull the first regular-expression match out of each
  cell into a new column (for example `#(\d+)` to grab an order number).
  Cells that don't match are left empty.
- **Replace in column** - find and replace within a single column's cells,
  using Plain, Wildcard, or Regex matching (same modes as the search bar).
- **Repair garbled characters** - fix text that was read with the wrong
  character set and saved that way, so a column shows MÃ¼ller instead of
  Müller. Octa only changes a cell when it can prove the repair by reversing
  the byte round-trip; anything it cannot prove is left exactly as it is,
  because a wrong "repair" is worse than the corruption. The same check drives
  the Clean-up suggestions panel, which finds these columns for you.
- **Tidy ID format** - write every valid IBAN, card number, barcode, VAT
  number or email address in a column one standard way (IBAN in capitals and
  groups of four, VAT without spaces, email domain in lower case). The dialog
  says how many values change and how many are not valid; invalid values are
  never touched. See Data Validation for what "valid" means.

Split, Merge, and Extract create new columns; Fill, Replace, Repair
garbled characters and Tidy ID format rewrite the chosen column in place. For the column-creating operations you can set the
new column name and the insert position (leave either blank for the default
shown as the field hint); for Split the name is used as a base, so the parts
become name_1, name_2, and so on. None of them change column types beyond
producing text, and all changes can be undone before you save.

## Conditional column (if / else-if / else)

**Columns > Conditional column...** builds a new column whose value depends
on conditions, like a spreadsheet IF/IFS or a SQL CASE. Add an ordered list of
rules such as "if amount > 100 then high, else if amount > 50 then medium,
else low". Each condition tests one column with an operator (equals, contains,
greater than, is empty, ...), and a rule writes its Then value when it
matches.

A rule can hold several conditions: click **Add condition** and choose **and**
(every condition must hold, for example amount > 100 and region = west) or
**or** (one of them is enough).

A condition can compare against several values, one per line: click **+**
for another line, or open the drop-down under the values to pick them from the
values that occur in that column (most common first, with how often each
appears; click one again to take it out). The condition holds when the cell
matches any of the values, for example region equals west or east. For
"does not equal" and "does not contain" it holds when the cell matches none
of them.

Rules are checked top to bottom and the first match wins (that is the
"else if" behaviour); reorder them with the ^ / v buttons. If no rule
matches, the Else value is used. With **Column type** on Automatic, outputs
that look like numbers become numeric cells and everything else is text.
Pick a type there (Text, Whole number, Decimal number, Boolean, Date, Date
and time) to choose it yourself; values that do not fit keep their text and
are flagged, so F10 walks them. The result is a new column (name and
position configurable) and is undoable with Ctrl+Z.

This shares its operators with Conditional formatting; the difference is
that conditional formatting colours matching cells, while a conditional
column sets a value.
"#;

pub const ANONYMIZE: &str = r#"# Anonymise Columns

**Data > Anonymise columns...** (Ctrl+Shift+Y) prepares a file for sharing by
masking or scrambling sensitive columns. Add rules, pick a strategy for each,
choose where the result goes, and press Apply. An Apply is a single undo step
(Ctrl+Z reverts the whole operation at once).

## Strategies

- **Hash** - replace each value with a stable hex code. The same value always
  hashes to the same code, so the data stays join-able.
- **Partial mask** - keep the first or last N characters and replace the rest
  with a mask character (for example ***-***-1234). Tick **Same length for
  all** to use a fixed number of mask characters for every cell, so the output
  no longer reveals how long the original value was. Left off, it masks exactly
  the hidden characters.
- **Redact** - replace the whole value with a fixed token ([REDACTED]) or an
  empty (null) cell.
- **Fake** - substitute realistic synthetic data (name, email, city, company,
  phone, UUID). Deterministic, so duplicates stay consistent.

A rule can target several columns; for mask / redact / fake the strategy is
applied to each.

## Hashing: SHA-256 vs BLAKE3

Both produce a 256-bit digest written as 64 hex characters. SHA-256 is the
widely known standard; BLAKE3 is a modern hash that is much faster on large
files. For masking either is fine and the result is equally join-able - pick
SHA-256 for familiarity, BLAKE3 for speed.

By default Octa writes the full 64-character hash. Turn off "Output full hash"
to keep only the first N characters as a shorter ID; the fewer characters, the
higher the (still small) chance two different values share a code.

## Salt

The optional **salt** is mixed into every value before hashing. The same value
plus the same salt always gives the same result, so duplicates stay linked and
a re-run with the same salt re-joins to an earlier export. A non-empty salt
makes the output non-guessable. Null and empty cells always pass through
unchanged.

## Combine columns into one ID

Select several columns in one **Hash** rule to hash them together into one new
column (a pseudonymous key), for example first + last into person_id. A
multi-column hash always creates a new column rather than overwriting.

## Output

- **Replace the columns in place** - overwrite the chosen columns.
- **Add the result as new columns** - keep the originals and append the
  anonymised values (e.g. email_anon).
- **Put a sanitised copy in a new tab** - leave the original untouched.

## Command line and assistant

The same engine is available as octa --anonymize spec.json data.csv (a JSON
spec file lists the rules, salt, and output mode) and as the anonymize MCP /
assistant tool.
"#;

pub const MARKING: &str = r#"# Colour Marking

Right-click a **cell**, **row number**, or **column header** to open the
context menu, then use the **Mark** submenu. Available colours: Red, Orange,
Yellow, Green, Blue, Purple, Pink, Teal, Lime, Indigo, Brown, Grey.

The **Edit > Mark** menu, and the **Mark** keyboard shortcut (default
**Ctrl+M**), apply a single colour to the **whole current selection**: a row
block, column block, multi-cell selection, or single cell. The shortcut uses
the colour set under **Settings > Table > Default mark colour** (Yellow by
default).

Mark precedence: cell > row > column. To clear a mark, right-click and choose
**Clear Mark**.

Marking works in **read-only mode** and on a read-only database tab. A
colour mark is a way of reading a table, not an edit of it: nothing is
written to the file or the server.
"#;

pub const CONDITIONAL_FORMAT: &str = r#"# Conditional Formatting

Where colour marking is something you apply by hand, conditional formatting
colours cells **automatically** based on their value, like the feature of the
same name in a spreadsheet. Open it via **Columns > Conditional formatting...**.

## Rules

The dialog holds a list of rules. Each rule has four parts:

- **Column** - a specific column, or `(any column)` to test every cell.
- **Operator** - `equals`, `does not equal`, `contains`, `does not contain`,
  `greater than`, `less than`, `greater or equal`, `less or equal`,
  `is empty`, `is not empty`.
- **Value** - the text or number to compare against (ignored by the two
  `empty` operators).
- **Colour** - which of the twelve mark colours to paint matching cells.

Tick **Aa** on a rule to make its text comparison case-sensitive. The
comparison is numeric when both the cell and the value look like numbers
(so `greater than 100` works as you'd expect), otherwise it compares text.

## How rules combine

Rules are checked from top to bottom and the **first** one that matches a
cell wins (like an if / else-if / else chain), so put your most specific
rules first. Use the **^** / **v** buttons on a rule to move it up or down
and build that order. A manual colour mark on a cell always takes priority
over a conditional rule.

Rules apply live as you edit them and update instantly when you change cell
values. They are **per tab and session-only** - they are not saved with the
file and do not change the data, only how it is shown. **Add rule** appends a
new row; the **x** button removes one; **Clear all** removes them all.

To set a cell **value** (rather than a colour) from conditions, use
**Conditional column** instead - see the Transform Column help.
"#;

pub const VALIDATION: &str = r#"# Data Validation

Data validation flags cells that break a rule you define, painting each
failing cell **red** so problems stand out. Open it via
**Data > Data validation...**.

## Rules

The dialog holds a list of rules. Each rule has a column (a specific
column, or `(any column)` to check every cell) and a kind:

- **Not empty** - the cell must have a value.
- **In range** - the cell must be a number within an optional **min** and
  **max** (leave a bound blank to leave that side open). A non-numeric
  cell fails.
- **Matches pattern** - the cell text must match a regular expression.
- **Unique** - every value in the column must be distinct; duplicated
  cells fail.
- **Max length** - the cell text must be at most the given number of
  characters.
- **IBAN**, **Card number**, **EAN / ISBN / UPC**, **VAT number (EU)**,
  **Email address** - the cell must be a correctly built ID of that kind:
  country, length and check digits for an IBAN, the check digit of a card
  number or barcode, the country's shape and (where published) check digit
  for a VAT number, the spelling of an email address. Spaces and dashes are
  fine; empty cells pass.
- **Compare with column** - the cell must compare as chosen (equal, not
  equal, less than, at most, greater than, at least) with another column in
  the same row, for example end at least start. Numbers compare as numbers;
  dates and times compare by time, a date counting as midnight; texts compare
  as text, case counting. Values of different kinds fail. An empty cell on
  either side passes. The rule needs a named column.
- **One of these values** - the cell must be exactly one of the values you
  choose. Press **Values (n)...** to see the column's distinct values, most
  common first, with counts and tick boxes; **Find** narrows the list and
  **Add value** takes a value not in the data yet. Matching is exact: case and
  trailing spaces count. Empty cells pass. The list shows at most 1,000
  values; a column with more has only the Add field. On a partly loaded
  table (database tab, large file) the list covers the rows loaded so far.
  Values already in the rule but absent from the data stay listed and can be
  unticked.

Rules point at columns by position while the tab is open, so after deleting
or moving columns check that they still name the columns you meant.

The footer shows a live count of how many cells currently fail.

## Checking IDs

**Valid means correctly built, not that it exists.** Every check is arithmetic
on the value itself: nothing is looked up and nothing leaves your computer. A
correct IBAN can belong to a closed account; whether a VAT number is registered
only the EU's online VIES service knows, and asking it would send your data out.
Phone numbers are not checked: that needs every country's numbering plan, and a
half-right check would paint good numbers red.

**A red cell never stops you.** It is a flag, not a gate. You can edit, save and
share the table as it is; what to do about it is your call.

Keep code columns as text: a barcode like 036000291452 read as a number loses
its leading zero and then fails. **Transform column > Tidy ID format** writes
every valid ID one standard way (IBAN in capitals and groups of four, and so
on) and leaves invalid ones exactly as they are.

## How it behaves

Rules apply live: failing cells are highlighted as soon as you add or edit
a rule, and the highlight updates when you change cell values. Validation
highlighting is **per tab and session-only** - it is not saved with the
file and does not change the data, only how it is shown. A manual colour
mark or a conditional-formatting colour takes priority over the red
validation highlight. **Add rule** appends a new rule; the **X** button
removes one; **Clear all** removes them all.
**Reusing a rule set.** Rules live with the tab and disappear when it closes,
which is fine while exploring and useless once the same check has to run every
week. **Save rules...** writes them to a TOML file and **Load rules...** reads
one back. Rules are stored by column name, not position, because a rules file
outlives the table it was written from. A compare rule is written as
kind "compare" with op (one of =, !=, <, <=, >, >=) and other_column; a pick
rule as kind "one_of" with values. If a loaded file names a column this
table does not have, the dialog says which ones rather than dropping them
quietly: a rules file that half applies is worse than one that fails loudly.
The same file runs as `octa --check FILE --rules RULES.toml`, which exits 1 on
any violation and on any rule that could not run, and as the `check_rules` MCP
tool. A compare failure's report names both values, for example
end=2024-01-01, start=2024-02-01.
"#;

pub const IMPUTE: &str = r#"# Fill Missing Values

Fill Missing Values replaces empty or null cells in one column using a
strategy you pick, so you don't have to fill gaps by hand. Open it via
**Data > Fill missing values...**.

## Strategies

- **Mean** / **Median** - fill with the average or middle value of the
  column's numbers (numeric columns only).
- **Mode** - fill with the most common value.
- **Constant** - fill with a fixed value you type.
- **Forward fill** - copy the nearest non-empty value from above.
- **Backward fill** - copy the nearest non-empty value from below.

Only empty/null cells are changed; existing values are left alone. Apply
writes the result back as a single undoable step. A strategy that doesn't
fit the data (for example Mean on a text column) shows an inline error and
changes nothing. Also available as `octa --impute` and the `fill_missing`
assistant/MCP tool.
"#;

pub const RENAME_COLUMNS: &str = r#"# Rename columns

**Columns > Rename columns...** renames many columns at once, instead of editing
each header by hand.

## The list

When you open it, the box is pre-filled with every column of the active tab, one
name per line. To rename a column, add a comma (or a tab) and the new name to its
line; leave a line unchanged to keep that column's name:

```
id,user_id
dob,date_of_birth
amount
```

Here `id` and `dob` are renamed and `amount` is left as it is.

As you edit, a live preview shows:

- **Will rename** - lines whose old name was found.
- **Not found** - old names that do not match any current column.
- **Collisions** - a new name that clashes with an existing column or is used
  twice. Apply stays disabled until you resolve them.

**Load from file...** appends more lines from a text file. Applying renames every
matched column as one step, so a single Undo reverts the whole batch.

## Duplicate column names

A file can name two columns the same thing, and then the list above cannot
help: it finds a column by its name, and a repeated name picks out the wrong
one. **Fix duplicate names** in the same dialog handles those by position
instead.

Tick it and the preview lists what it would do. The first column keeps the
name; every later one gets a number: `id`, `id_2`, `id_3`. A suffix another
column already owns is skipped, so `a`, `a`, `a_2` renames only the middle one
(to `a_3`) and leaves the real `a_2` alone.

**Ignore upper/lower case** widens it, so `Name` and `name` count as the same
name and the second one gets numbered. Off by default, since some files mean
those as two different columns.

**Columns > Fix duplicate names...** is the same dialog opened straight onto
this half of it. Either way the renames join the same single undo step.

Duplicate names are worth fixing before you run SQL over the table, join it or
export it: those all address a column by name.
"#;

pub const CLEAN_HEADERS: &str = r#"# Clean Headers on Load

Clean Headers on Load is an optional setting that tidies column names the
moment a file opens, turning headers like `First Name` or `E-mail Address`
into lower snake_case identifiers (`first_name`, `e_mail_address`). Enable
it under **Help > Settings > Clean headers on load**.

## What it does

Each header is trimmed, lowercased, and has spaces and punctuation
replaced with single underscores; leading and trailing underscores are
stripped. Duplicate results get a numeric suffix (`name`, `name_2`) so
every column keeps a distinct name. A header that has no usable characters
becomes `column`.

It is off by default, so files load with their original headers unless you
opt in. It pairs naturally with **Trim whitespace on load**.
"#;

pub const PARTITION: &str = r#"# Split into Files

Split into Files writes the active table as several files: one per value of
a column, at most so many rows each, at most so much size each, or a mix.
Open it via **Data > Split into files...** (Ctrl+Shift+Z).

## The three limits

Choose at least one of these; they combine.

- **Column** - one file per distinct value, named after the value. Splitting a
  sales table by `region` gives `north.csv`, `south.csv`, and so on. Leave it
  at **(none)** to split the whole table by rows or size only.
- **Max rows per file** - no file holds more rows than this. Type a whole
  number; `10,000` works.
- **Max size per file (MB)** - no file is bigger than this (1 MB = 1,000,000
  bytes; decimals such as `2.5` work). Octa writes a file, measures it and
  rewrites it with fewer rows if it came out too big, so every format works,
  compressed Parquet included. Files land a little under the limit rather than
  packed to the byte.

With a column and a limit, each value is split further. With only a limit the
files are named after the source file.

## Names

Without a row or size limit the names are the plain ones below. With a limit
every file gets a part number: flat files become `berlin_part_0001.csv`,
`berlin_part_0002.csv`, ..., and the folder layouts number the files inside
their folder (`city=Berlin/part-0001.csv`). The numbers count per folder, so
each folder starts at `part-0001`. The preview in the dialog uses the same
naming code as the writer and shows the first file of the first few values.

## Big tabs

If the tab holds only part of its source, the dialog says so and offers
**Load the rest**; split writes only the rows that are loaded. A tab that
shows one page of a very large file is refused with a pointer to
`octa --split-rows` and `octa --split-size`, which stream the whole file.

## Running

The split runs in the background with a progress line and **Cancel**. Cancel
stops after the file being written; files already written stay.

The original table is not changed. Also available as `octa --partition-by`,
`--split-rows`, `--split-size` and the `partition_table` assistant/MCP tool.

### Choosing how the pieces are named

The **Layout** option offers four shapes. Taking `city` with values `New York`,
`Berlin` and `Sao Paulo`, and writing CSV:

- **Flat files** - `new_york.csv`, `berlin.csv`, `sao_paulo.csv`
- **Folder per value** - `New York/part-0001.csv`, `Berlin/part-0001.csv`
- **Hive folders** - `city=New York/data.csv`, `city=Berlin/data.csv`
- **Hive folders, numbered files** - `city=New York/part-0001.csv`

**All four hold the same rows**, and all four reopen as one table with
**File > Open table folder...**, because the column you split on is written
into every file whichever you pick. The choice is only about the names - but
the names are not merely cosmetic.

**Flat** is the only lossy one: the value goes through the tidy-up a SQL
identifier gets, so capitals fold to lower case and anything that is not a
letter or digit becomes an underscore. `New York`, `new-york` and `NEW_YORK`
all arrive as `new_york`, and the second and third become `new_york_2.csv` and
`new_york_3.csv`. No rows are lost, but the name stops telling you which value
is inside. An empty value becomes `table.csv`, one starting with a digit gains
a `t_` prefix.

**The three folder layouts keep the value intact**, replacing only characters
that cannot appear in a path. **Hive** (`column=value`) is what Spark, Athena,
DuckDB and pandas expect from a partitioned dataset, so pick it when the files
go into another tool; the **numbered** variants name the file `part-0001`
rather than after the value or `data`, which is what those tools write
themselves and some pipelines expect.

You do not have to work this out from the descriptions. The dialog shows a
**live preview** of the first few paths, built from your own column's values
with the same code that writes the files, and it follows every change to the
column, the format and the layout.

The layouts only apply when you split by a column. Every option explains
itself on hover. Flat is the default, so nothing changes unless you pick
otherwise.
"#;

pub const OUTLIERS: &str = r#"# Detect Outliers

Detect Outliers highlights numeric values that sit far from the rest of
their column, or from their neighbours in a series over time, painting each flagged cell **orange** so unusual readings
stand out. Open it via **Analyse > Detect outliers...**.

## Methods

- **IQR (interquartile range)** - flags cells outside
  `[Q1 - k*IQR, Q3 + k*IQR]`. The usual `k` is `1.5`.
- **Z-score (standard deviations)** - flags cells whose value is more than
  `k` standard deviations from the mean. The usual `k` is `3`.
- **Neighbours (for a series over time)** - flags cells that differ from the
  values just before and after them. See below.

Tick the columns to scan (numeric columns are pre-selected) and set `k`,
then press **Detect**. Columns with fewer than four numbers are skipped.

## Unusual points in a series over time

IQR and Z-score look at a column as a whole. In a series that grows, or that
has a busy season, that hides exactly the values worth a look: a bad day in
December is still higher than a good day in February, so neither method
flags it. **Neighbours** compares each value with the values around it
instead.

- **Order by** is the date, time or number column that puts the rows in
  order. The rows need not be sorted, and a row without a value there is
  left out.
- **Points each side** is how many values before and after are compared.
  Empty means 7, so each value is judged against up to 14 neighbours. It
  needs at least four.
- **Same weekday only** compares a Sunday with the Sundays before and after
  it, a Monday with Mondays. Use it for daily data with a weekly rhythm,
  where a quiet Sunday is normal and a quiet Tuesday is not. The points each
  side then count weeks.
- A value is flagged when it lies more than `k` robust deviations from its
  neighbours' median. The usual `k` is `3`. The deviation is 1.4826 times
  the median distance from that median, so one extreme neighbour does not
  widen it. Where the neighbours are all the same, any other value is
  flagged.

An example: visitors per day, **Order by** `day`, **Points each side** 4.

| day        | visitors |
|------------|----------|
| 2024-06-05 | 103      |
| 2024-06-06 | 99       |
| 2024-06-07 | 240      |
| 2024-06-08 | 102      |
| 2024-06-09 | 97       |

The days around 7 June all lie near 100, so 240 is flagged and nothing else
is. In a series that climbs from 100 to 1,000 over a year, the same jump on an
early day would be flagged here too, while IQR and Z-score would let it pass
as an ordinary value of the column.

The three controls are greyed for the other two methods. The order column
itself is never scanned.

## What Detect does

Choose under **When done**:

- **Highlight outlier cells** - paints each flagged cell **orange**. This is
  **per tab and session-only**: it never changes the data, only how it is
  shown, and **Clear highlight** removes it. Manual colour marks, conditional
  colours, and validation highlights all take priority over the orange.
- **Add an is_outlier column** - appends a boolean `is_outlier` column that
  is `true` for every row holding at least one flagged value. This is a real,
  undoable edit (Ctrl+Z) you can save, sort, or filter on.

Also available as `octa --outliers` and the `detect_outliers` assistant/MCP
tool (both report the flagged cells).
"#;

pub const PII: &str = r#"# Detect PII

Detect PII scans the table for columns that look like personal data, so
you can find sensitive fields before sharing a file. Open it via
**Analyse > Detect PII...**.

## How it works

Octa weighs two clues for every column:

- the **column header** (does it look like `email`, `first_name`, `gender`,
  `country`, `birthdate`, `ip`, ...?), and
- the **cell values** (how many match a known shape: email, phone, IP
  address, credit card, IBAN, SSN, date, postal code).

This is why fields with no give-away values, like names, gender or country,
are still found from their header, while a plain number column like
`salary` is left alone.

## Confidence

The percentage combines those two clues:

- a strong value pattern on its own reaches at least 60%,
- a matching header on its own reaches 60%,
- the two together score highest (up to 100%).

A column is listed when its best guess is at least 50%. The **Basis** column
tells you which clue drove it: `column name`, `values (N%)`, or both.

**Send to Anonymise** opens the Anonymise dialog pre-filled with one hashing
rule per detected column. Also available as `octa --detect-pii` and the
`detect_pii` assistant/MCP tool, which return the same `confidence`,
`by_name` and `value_match` fields.
"#;

pub const HASH_COLUMNS: &str = r#"# Hash Columns

**Columns > Hash columns...** adds one column holding a hash of the columns
you pick, row by row. It is the usual way to build a hash key in a data
warehouse, for example `MD5(UPPER(TRIM(first_name)) || '|' || birth_date)`.

Any column type works. Each value is turned into text first, the way Octa
shows it in the grid, then the values are joined and hashed.

## Options

| Option | What it does |
|---|---|
| **Columns, in order** | Pick columns with **Add a column**; move them with `^` / `v`, take one out with `x`. The order is part of the hash: `a, b` and `b, a` give different results. |
| **Algorithm** | MD5 (32 hex characters, the default), SHA-256 (64) or SHA-512 (128). |
| **Delimiter** | Put between the values, `\|` by default. Any text, also empty. Without one, `ab` + `c` and `a` + `bc` hash the same, which is why a delimiter is the default. |
| **NULL as** | The text that stands in for an empty cell. Empty by default; some teams use a marker such as `<NULL>` so that an empty cell and an empty string differ. |
| **Trim whitespace** | Strip spaces at the start and end of each value first. |
| **Upper-case** | Upper-case each value first, so `abc` and `ABC` hash the same. |
| **New column** | The name of the result, `hash_<columns>` by default. It must not exist yet. |

The **Preview** shows the first rows' hashes. Hover one to see the exact text
that was hashed. The hash is lowercase hex. **Edit > Undo** removes the
column again.

## On a live database

On a database tab that holds only part of its table, the database computes
the hash, for every row:

- **Add column** reads the table again from the database with the hash
  filled in, so the selection, marks and **Undo** history start fresh. Every
  page you scroll to, and **Load whole table**, carries the hash too. Save or
  undo your changes in the tab first; until then **Add column** is greyed.
- The **Preview** comes from the database: a short pause after you change a
  setting, it shows the hashes of the first rows the database returns.
- Analyses that run on the database (Summary, Value frequency, the filters,
  sorting) see the hash column over every row, so finding duplicate keys
  covers the whole table.
- The hash cells cannot be typed into. Other changes to them, such as Find
  and replace, are never saved, and the next read from the database puts the
  database's hash back. Delete the column to remove it; **Refresh** reads the
  table fresh, without it.
- Each value is the database's own text of the cell. Dates and decimals can
  look different from the grid, so a hash can differ from the same row in a
  file. **Trim whitespace** strips spaces only on the database. SQL Server
  needs version 2019 or later (UTF-8 text).
- If the database refuses, the table shows why, with **Try again** and **Use
  the loaded rows**; with the loaded rows the hash column stays empty. So it
  does after switching **Run analyses on the database** off, which reads the
  plain table again.

## Good to know

- A tab that holds only part of its source (a file cut at the row limit, or a
  database table with **Run analyses on the database** switched off) cannot
  be hashed yet: the result would cover the loaded rows only. Use **Data >
  Load whole table...** first.
- A database renders dates and decimals as text its own way, so the same
  values can hash differently in Octa and in a database query. When the two
  must match, cast to text yourself in SQL in the same format Octa shows.
- The same function is available from the command line as
  `octa --hash-columns` and to AI agents as the MCP tool
  `hash_columns`.
"#;

pub const HEADER_ROW: &str = r#"# Header row

Some files do not start with their column names. Bank and government exports
often put a title block above the real header, Excel reports spread the
header over two rows with merged cells, and some files have no header at
all. Octa opens them as they are and lets you fix the header afterwards.

## Use a row as the header

Right-click any cell in the row that holds the column names and pick **Use as
header row**. That row becomes the column names, and it and every row above
it are removed.

When the header spans two rows (a merged `2025` above `Q1 Q2 Q3 Q4`), select
both rows first: click the first row number, Shift+click the second, then
right-click. The entry reads **Use rows 4-5 as header** and combines them. A
blank cell in the upper row takes the text to its left, so the merged `2025`
reaches every quarter below it and the names become `2025 Q1`, `2025 Q2`, and
so on.

Blank names become `column_1`, `column_2`, ...; a name that repeats gets
`_2`, `_3`.

Afterwards Octa looks at every column again, the way it does when a file
opens: a column that was text only because of the title rows becomes numbers
or dates again, European numbers like `1.234,56` included. A column that
could be read two ways, such as dates that work day first and month first,
stays text; **Change column type** lets you pick.

**Ctrl+Z** puts everything back in one step: the rows, the names and the
types.

## Octa suggests the row

When most column names are placeholders such as `column_3` or `Column3`, the
file almost certainly has a title block. Octa then looks through the first
20 rows for one that is filled across and made of words, and shows a banner:

> The column names look like they are in row 4. **Use row 4 as header**
> **Dismiss**

Nothing changes until you click. For a merged two-row header the banner
offers both rows.

## A file without a header

If the first row of the file is data, Octa has used it as the column names.
Untick **Edit > First row is header**: the names go back into the table as
the first row, and the columns are called `column_1`, `column_2`, and so on.
Tick it again, or press Ctrl+Z, to undo that.

## When it is not available

- In read-only mode and in large-file mode, like every other edit.
- On database tables: their column names come from the database.
- While a filter hides rows: rows above the header that you cannot see would
  be removed. Clear the filter first. A sort is fine, because the rows above
  are then the rows you see above.

## Next month's file

Both actions are recorded as Recipe steps (`use_header_rows`,
`header_is_data`), so replaying the recipe on next month's export fixes its
header the same way: in Octa, with `octa --recipe`, and over MCP. The step
remembers the row number, so it fits files whose title block has the same
height.

## Title lines in CSV files

A CSV whose first line is a title (`Report generated 03.10.2026`) used to open
one column wide, because every row was sized to that line. Octa now sizes the
table to the rows below it and finds the separator from the lines that agree
with each other, so a `;` file with a title line opens with every field in
place. Columns that are empty in every row, because each line ends in a
separator, are left out as before.
"#;

pub const CALCULATED_COLUMNS: &str = r#"# Calculated columns

**Columns > Calculated column...** adds one column that holds a rank, a
running total, a share of the total, the change to the previous row, a
neighbouring row's value, a visit number or an ABC class. It is
the quick way to answer "who is first in each region?" or "how much have we
sold so far?" without writing a formula or a SQL window function.

The new column is inserted beside the value column, or at the left edge for
Row number, which has no value column. **Edit > Undo** removes it again.

## The eleven kinds

| Kind                               | What the new column holds                                                                         |
|------------------------------------|---------------------------------------------------------------------------------------------------|
| **Rank**                           | The place of each row by its value. Ties share a place and leave a gap (1, 2, 2, 4).              |
| **Dense rank**                     | Like Rank, but ties leave no gap (1, 2, 2, 3).                                                    |
| **Row number**                     | 1, 2, 3 in the chosen order. Ties keep the table order. Needs no value column.                    |
| **Running total**                  | The sum of the value up to and including each row, in the chosen order.                           |
| **Percent of total**               | Each value as a percentage of its group's total. 12.5 means 12.5 percent.                         |
| **Difference to previous row**     | The value minus the previous row's value, in the chosen order.                                    |
| **Percent change to previous row** | How much the value changed from the previous row, in percent. Empty when the previous value is 0. |
| **Value from previous row**        | The value a number of rows before, in the chosen order. Empty at the start of a group.            |
| **Value from next row**            | The value a number of rows after, in the chosen order. Empty at the end of a group.               |
| **Visit number from timestamps**   | 1, 2, 3 per user: a new visit starts after a pause longer than the gap. See below.                |
| **ABC class**                      | A, B or C by the running share of the total, largest value first. See below.                      |

Running total, Percent of total, Difference, Percent change and ABC class do
arithmetic, so the value column must hold numbers. Numbers stored as text are read as
numbers and blank cells are allowed; text that does not read as a number is
refused.

## Options

| Option               | What it does                                                                                                                                                                                                |
|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Kind**             | One of the eleven kinds above.                                                                                                                                                                               |
| **Value column**     | The column the calculation works on. Not used by Row number.                                                                                                                                                |
| **Order by**         | The row order for Row number, Running total, Difference and Percent change. **(table order)** keeps the rows as they are. Rank and Dense rank order by the value itself, and Percent of total has no order. |
| **Largest first**    | Reverses the order, largest or latest value first. On by default for Rank and Dense rank, so the largest value gets 1.                                                                                      |
| **Restart for each** | Columns whose values restart the calculation, for example per region. Any number of columns.                                                                                                                |
| **Rows away**        | For the previous and next row's value: how many rows before or after. Empty means 1. |
| **Gap in minutes**   | For the visit number: a pause longer than this starts a new visit. Empty means 30. |
| **A up to percent**, **B up to percent** | For the ABC class: where class A and class B end, as a running share of the total. Empty means 80 and 95, the classic 80 / 15 / 5 split. |
| **New column name**  | The name of the result. It must be free: the default never clashes, and a name you type that is taken is refused. |

The **Preview** shows the first rows with the new column.

## A worked example

Take this table, in this row order, with **Restart for each** set to
`region` and **Order by** left at table order:

| region | amount | Rank | Running total |
|--------|--------|------|---------------|
| North  | 30     | 2    | 30            |
| North  | 50     | 1    | 80            |
| North  | 20     | 3    | 100           |
| South  | 40     | 1    | 40            |
| South  | 10     | 2    | 50            |

Rank counts the largest amount as 1 within each region. The running total adds
up row by row and starts again at the first South row. Without a group, the
whole table counts as one group.

## Value from the previous or next row

The new column repeats the value column, shifted. With **Order by** set to a
date and **Restart for each** set to a customer, **Value from previous row**
puts each customer's previous order beside the current one, and **Value from
next row** the following one. The result keeps the value column's type, so a
date stays a date. **Rows away** reaches further: 12 on monthly rows is the
same month a year earlier. **Order by** and **Largest first** apply as for the
two changes.

| customer | ordered_at | amount | Previous amount |
|----------|------------|--------|-----------------|
| ann      | 2024-01-10 | 20     |                 |
| ann      | 2024-02-05 | 35     | 20              |
| ann      | 2024-03-02 | 15     | 35              |
| bob      | 2024-01-22 | 50     |                 |
| bob      | 2024-03-14 | 40     | 50              |

Each customer's first order has nothing before it, so it stays empty. Without
**Restart for each**, Bob's first row would show Ann's 20, the order before it
by date.

## Visit numbers from timestamps

Pick the column that holds the date and time as **Value column**, the user or
device column under **Restart for each**, and the kind **Visit number from
timestamps**. Each user's rows are put in time order and numbered: the first
row is visit 1, and the number goes up by one whenever the pause since that
user's previous row is longer than the **Gap in minutes**.

| user | at               | Visit |
|------|------------------|-------|
| ann  | 2026-03-01 10:00 | 1     |
| ann  | 2026-03-01 10:20 | 1     |
| ann  | 2026-03-01 11:30 | 2     |
| ben  | 2026-03-01 10:05 | 1     |

A pause of exactly the gap still belongs to the same visit. The rows need not
be sorted, and the table's row order does not change. A date without a time
counts as midnight, a plain number counts as seconds, and text that is no date
or time is refused. A row without a timestamp gets no number.

## ABC classes

The rows are sorted by the value, largest first, and each gets a class by the
share of the total that the rows before it already cover: **A** while that
share is below 80 percent, **B** below 95 percent, **C** after that. So the
few rows that make up most of the total are the A rows. The row that crosses a
limit still belongs to the class it started in, which makes the largest row
always an A. Type other limits into **A up to percent** and **B up to
percent** when the classic 80 / 15 / 5 split does not fit; A must be below B
and both between 0 and 100.

| product | revenue | share before | ABC class |
|---------|---------|--------------|-----------|
| P1      | 50      | 0            | A         |
| P2      | 30      | 50           | A         |
| P3      | 10      | 80           | B         |
| P4      | 5       | 90           | B         |
| P5      | 4       | 95           | C         |
| P6      | 1       | 99           | C         |

A negative value is refused, since a share of a total with negative parts
means nothing. With **Restart for each**, every group gets its own total and
its own classes. To see the same thing as a picture, use the **Pareto** kind
of the Chart.

## Empty values

An empty cell is a null or blank text.

- **Rank**, **Dense rank** and **Row number** put empty values last.
- **Running total** adds nothing for an empty value and carries on.
- **Difference** and **Percent change** are empty when either side is empty,
  and at the start of each group.
- **Percent of total** leaves an empty cell empty. It is also empty when its
  group's total is 0.
- **Value from previous row** and **Value from next row** hand an empty cell
  on as it is.
- **Visit number** and **ABC class** leave a row with an empty value empty and
  do not count it. ABC classes are also empty when the group's total is 0.

## Recipes

The column is recorded in a recipe as a "calculated" step, by column
name, so it can be replayed on another file with the same columns.

## On a live database

On a database tab that holds only part of its table, the database computes the
column over every row, with window SQL, and the table is read again with it.
Because the server changed the data, **Undo** cannot take that back. Save or
undo your changes in the tab first.

- Only NULL counts as empty on a server, so blank text ranks as a value there.
- Text ranks follow the database's collation, which is case-blind on MySQL and
  SQL Server. A file compares text exactly.
- ClickHouse has no suitable window function for the two changes and the
  neighbouring row's value, so those four kinds are computed on the loaded
  rows there.
- A database has no row order of its own, so Row number, Running total,
  Difference, Percent change and the previous or next row's value need an
  **Order by** column on a server.
  Without one they are computed on the loaded rows only, and the status line
  says so.
- A visit number is always computed on the loaded rows, with the note.
- An ABC class is computed by the server, which does not refuse negative
  values as a file does. Rows with the same value may fall on either side of
  a limit there.
- If the spec uses a column the server added itself, or the engine cannot
  express it, the column is also computed on the loaded rows with a note.

## Command line and AI agents

The same calculation is available as `octa --calculate` on the command line
and as the MCP tool `calculate_column`, which the Assistant uses too.
"#;

pub const NUMBER_BANDS: &str = r#"# Number bands

**Columns > Number bands...** works both ways round, and adds the result as a
new column right after the one you pick. One Undo takes it away again.

**Numbers to bands** turns numbers into bands such as `18-34` and `65+`:

- **Equal widths**: bands of the same width from the smallest to the largest
  number. Whole numbers get a whole width.
- **Equal counts**: about as many values in each band. 4 gives quartiles,
  10 deciles. Equal values cannot be split, so fewer bands can come out.
- **Own edges**: type where bands start, such as `18, 35, 65`, which makes
  `<18`, `18-34`, `35-64` and `65+`. Use semicolons when the numbers have
  decimal commas.

In a column of whole numbers a label names both ends (`18-34` holds 34);
other numbers get interval labels such as `[1.5, 3)`.

**Bands to numbers** reads band labels and gives the middle, the lower end or
the upper end of each. Labels are read the way people write them: `18-24`,
`18 to 24`, `18 bis 24`, `[18, 25)`, `10,000-19,999`, `20k`, `65+`, `<18`. A
lone number with words (`65 and over`, `unter 18`) sits at the end of the
other bands it is next to. An open band takes the width of the band beside
it, so `65+` beside `55-64` reads as 65 to 74. A label with no number stays
empty, and the status line says how many.

Also on the command line as `--bands` and through MCP as `number_bands`.
"#;

pub const REFERENCE_DATA: &str = r#"# Reference data

**Data > Reference data...** adds columns that need public data from outside
your file. One dialog, three modes:

- **Convert currency** converts an amount at the exchange rate of the row's
  own date, so an invoice from March 2019 is converted at the rate of that
  day, not today's.
- **In today's money** expresses an amount from one month in the prices of
  another, by the consumer price index of its country. 100 euros from 2015
  become what they are worth now.
- **Places and region keys** looks up a postcode, a municipality code, a
  Kreisschlüssel or a NUTS code and adds the place, its coordinates and the
  other keys. The postcode `47877` in Germany gives the municipality
  `05166036` Willich, the Kreis `05166` Kreis Viersen and the NUTS 3 region
  `DEA1E`.

The new columns go right after the column they come from, and one Undo takes
them away again. The source column is never changed. A status line says how
many rows were found.

## Before you start

**Everything here is off until you switch it on.** Octa ships none of this
data. Open **Settings > Online data** and switch on the downloads you want;
Octa then fetches the files, keeps them on your computer and refreshes them at
the interval you set. Nothing about your data is sent.

| Mode                   | Needs                                                                           |
|------------------------|---------------------------------------------------------------------------------|
| Convert currency       | **Exchange rates**                                                              |
| In today's money       | **Inflation**                                                                   |
| Places and region keys | **Postcodes**, **European regions** or **German postcodes and municipalities**  |

The dialog always opens. A mode whose download is off is greyed, says which
switch it needs and offers **Open Settings**. In Places and region keys each
output is greyed on its own, so one switched-on source is enough to start.

## Convert currency

| Field                  | What it does                                                                                              |
|------------------------|-----------------------------------------------------------------------------------------------------------|
| Amount                 | The column with the amounts. Text holding numbers works too, European ones such as `1.234,56` included.   |
| Currency of the amount | **From a column** of codes (`EUR`) or names (`Euro`), or **The same for every row**.                      |
| Convert to             | The currency of the new column.                                                                           |
| Date                   | The column whose date picks the rate, or **Latest rate** for the newest rate on every row.                |
| Rounding               | **Usual decimals of the currency**, **Round to** a number of decimals, or **Do not round**.               |
| Currency shown as      | **Official code** or **Symbol**, for the currency column.                                                 |

Three columns are added: the converted amount `<amount>_<TO>`, the currency
`<amount>_<TO>_currency` and `<amount>_<TO>_rate_date`, which names the rate
that was used. The amount stays a plain number, so it still sums and sorts.

Which rate a row gets, in this order:

1. The daily rate of the row's date.
2. On a weekend or holiday, the last daily rate before it.
3. For a currency without daily rates, the average of that month. The rate
   date column then reads `2024-03 (monthly average)`.
4. For a date after the newest rate, the newest rate. The rate date column
   shows its date.
5. For a date before the first rate, an unknown currency or an empty amount,
   the cell stays empty and the rate date column says why.

The Bank for International Settlements quotes every currency against the US
dollar, so a conversion between two other currencies goes through the dollar:
the target's rate divided by the source's rate of the same date. The
arithmetic is exact decimal arithmetic, never floating point. When both
currencies are the same the amount is copied unchanged.

**Rounding.** The usual decimals are 2 for most currencies, 0 for the yen, the
won and the other currencies without a minor unit, and 3 for the dinars
(Bahrain, Iraq, Jordan, Kuwait, Libya, Tunisia) and the Omani rial. **Do not
round** keeps every digit the calculation gives, up to 20 decimals.

**Currency shown as.** The official code (`EUR`, `USD`, `JPY`) is the default
and clear in every language. **Symbol** writes `€` or `£` instead, but only
where the symbol belongs to one currency alone: € £ ₹ ₪ ₩ ₦ ₽ ₺ ₴ ₫ ₸ ₾ ₼ ₲ ₡ ₵
₭ ₮ and zł. `$` is shared by the US, Canadian and Australian dollar and many
more, `¥` by the yen and the yuan, and `kr`, `Fr.` and `₱` are shared too, so
those currencies keep their code.

A made-up example (these are not real rates): 100 EUR dated Saturday
2026-01-03, converted to USD, takes the rate of Friday 2026-01-02 and gives

| amount | amount_USD | amount_USD_currency | amount_USD_rate_date |
|--------|------------|---------------------|----------------------|
| 100    | 105.26     | USD                 | 2026-01-02           |

## In today's money

| Field            | What it does                                                                              |
|------------------|-------------------------------------------------------------------------------------------|
| Amount           | The column with the amounts.                                                              |
| Date             | **From a column** of dates, or **The same for every row**: one year and month.            |
| Country          | **From a column** of names or codes, or one country. The euro area counts as one.         |
| In the prices of | The month the amounts are expressed in. Default: the newest month every country has.      |
| Rounding         | **Round to** a number of decimals (default 2) or **Do not round**.                        |

The new amount is the old one, times the price index of the target month,
divided by the price index of the row's month. An amount of 100 from a month
with index 100, in the prices of a month with index 132, is 132.

Two columns are added: `<amount>_in_<month>` and `<amount>_index_month`, which
names the two months used (`2015-01 to 2026-01`). A country whose index ends
before the target month uses its newest month, and the index month column says
so. A date before the first month, or a country that is not covered, leaves
the cell empty with the reason.

The lists show currencies and countries by name, in the language Octa is set
to, with the code after it: `Euro (EUR)`, `Germany (DE)`.

Countries are recognised by name in many languages and by ISO code. The index
is each country's own consumer price index, monthly, so the result is that
country's inflation, in that country's currency.

## Places and region keys

| Field            | What it does                                                                             |
|------------------|------------------------------------------------------------------------------------------|
| Key column       | The column holding the postcodes or codes to look up.                                    |
| The column holds | **Postcodes**, **Municipality codes**, **Kreisschlüssel** or **NUTS codes**.             |
| Country          | **From a column**, or one country. Kreisschlüssel and NUTS codes carry their own country.|
| Add              | The columns to add, see below.                                                           |
| Several matches  | **All, joined** or **First only**.                                                       |

| Output                                       | Comes from                                                                |
|----------------------------------------------|---------------------------------------------------------------------------|
| Place, Region, Latitude, Longitude           | Postcodes (GeoNames)                                                      |
| Postcode (when you look up a code)           | German postcodes and municipalities (OpenPLZ), European regions elsewhere |
| Municipality code and name                   | OpenPLZ for Germany (exact), European regions elsewhere                   |
| Kreisschlüssel and Kreis name                | The first five digits of the municipality code; the name from GeoNames    |
| NUTS 1, NUTS 2 and NUTS 3, each code and name | European regions (Eurostat)                                               |

Each output becomes a column named `<key>_<output>`, such as `plz_place` or
`plz_nuts3`.

**All** and **None** tick or untick every column at once.

**Which key works where.** Postcodes work in every country listed under
Coverage. Municipality codes work in the 34 European countries Eurostat
covers; in Germany that code is the Gemeindeschlüssel. The Kreisschlüssel is
German only, and the dialog lists it only once German municipality codes are
stored (**European regions** or **German postcodes and municipalities**
downloaded). Other countries have no such key: use the NUTS 3 code there,
which in Germany names the same area as the Kreis.

**Several matches, both ways.** A postcode can lie in several places and
municipalities, and a municipality, Kreis or region holds many postcodes. In
Germany 1,757 postcodes lie in more than one municipality (`01945` covers 11),
and the Kreis `05166` holds 12 postcodes.

- **All, joined** (default) puts every match in one cell, sorted and separated
  by commas: `Guteborn, Ruhland, Tettau`. Coordinates are the centre of the
  matched places.
- **First only** gives the first match in alphabetical order, with its own
  coordinates.

OpenPLZ lists a few streets of `47877` under Mönchengladbach (`05116000`), so
with **All, joined** the municipality cell for that postcode holds both codes.

For a municipality code, Kreisschlüssel or NUTS code, **Place** lists the
places of the postcodes inside it and the coordinates are their centre.

**Octa tidies the keys before matching.** A postcode column that was read as
numbers gets its leading zeros back (`1945` becomes `01945`) wherever the
country's postcodes all have one length. Spaces and upper or lower case do not
matter, so `sw1a1aa` matches `SW1A 1AA`. Municipality codes and Kreisschlüssel
are padded the same way.

Coordinates mark the area of a postcode, not a street or a house.

## Coverage and known limits

- **Exchange rates**: 147 currencies. 61 have daily rates, most of them since
  about 1970; the others have monthly averages.
- **Inflation**: the monthly consumer price index of 63 economies and the euro
  area.
- **Postcodes**: the 124 countries and territories GeoNames publishes, with
  full postcodes for the United Kingdom, the Netherlands and Canada. Chile,
  China, Ireland and Malta have only the first part of the postcode, Argentina
  the first five characters, Brazil only the main codes.
- **European regions**: 34 countries, the EU 27 plus Switzerland, Iceland,
  Liechtenstein, Norway, North Macedonia, Serbia and Turkey. Greece is
  covered here although GeoNames lacks it.
- Outside Germany, Eurostat places each postcode at one point and names the
  municipality that point lies in. A postcode that crosses a municipal border
  therefore names only one municipality. For Germany, OpenPLZ gives every
  pair.
- Iceland has no sheet in Eurostat's municipality workbook, so its NUTS 3
  regions come from its postcodes.

## Where the data lives

In the `reference` folder next to Octa's settings file, as Parquet files:

| File                               | Holds                                              | About           |
|------------------------------------|----------------------------------------------------|-----------------|
| `currency_daily.parquet`           | One row per day, one column per currency           | 1.6 MB          |
| `currency_monthly.parquet`         | One row per month, one column per currency         | 0.4 MB          |
| `inflation.parquet`                | One row per month, one column per economy          | 0.1 MB          |
| `postcodes/<country>.parquet`      | Postcode, place, region, coordinates               | 0.25 MB to 7 MB |
| `eu_postcodes.parquet`             | Country, postcode, NUTS 3, municipality code       | 1.4 MB          |
| `eu_municipalities.parquet`        | Country, municipality code, name, NUTS 3           | 1 MB            |
| `nuts.parquet`                     | NUTS code and name                                 | 40 KB           |
| `de_postcode_municipality.parquet` | German postcode and municipality pairs             | 30 KB           |

They are ordinary Parquet files, so Octa can open them like any other table.
Rates and indices are stored as exact decimals.

Switching a download off keeps its data, and the dialog keeps working with
it. **Delete downloaded data** in **Settings > Online data** removes it.

## How refresh works

Each download has its own **Refresh every** interval, in days or months, one
month by default. When the interval has passed, Octa looks again at start-up
and before the dialog applies. Postcodes are fetched per country, the first
time a country is used.

- **Unchanged files are skipped.** Octa asks the server whether the file has
  changed and downloads it only if it has.
- **A new download never loses what you have.** A new value replaces a stored
  one only where the new download has a value; anything it lacks stays as it
  was. For postcodes the unit is one postcode: when the new download lists it,
  what it says replaces the old rows for that postcode.
- **A series on a new base is replaced whole.** If a price index moves from
  2010 = 100 to 2020 = 100, or a currency is redenominated, old and new values
  are on different scales and must not be mixed. Octa notices that every
  overlapping value differs by the same factor and takes that series entirely
  from the new download.
- **A failed download changes nothing.** A window names the source and the
  reason, with text you can select and copy, and the data downloaded before
  stays in use. Behind a proxy or firewall these hosts must be reachable:
  `data.bis.org`, `download.geonames.org`, `gisco-services.ec.europa.eu`,
  `ec.europa.eu` and `raw.githubusercontent.com`. **Download now** in
  Settings tries again.

## Sources and licences

| Download                            | Credit                                                                                                                                                  |
|-------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------|
| Exchange rates, Inflation           | Source: BIS (Bank for International Settlements), data.bis.org                                                        |
| Postcodes                           | GeoNames, CC BY 4.0. United Kingdom: Contains Royal Mail data (c) Royal Mail copyright and database right                     |
| European regions                    | (c) European Union - GISCO, postal code point dataset, Licence CC BY-SA 4.0; NUTS and municipalities (c) European Union, Eurostat                            |
| German postcodes and municipalities | OpenPLZ, from OpenStreetMap, (c) OpenStreetMap contributors, ODbL 1.0                                                       |

The dialog and the Settings section show the same credits.

What this means for you:

- **For your own use, nothing.**
- **When you pass a table with these columns on**, name the sources of the
  columns you added.
- Columns from **European regions** (CC BY-SA 4.0) or **German postcodes and
  municipalities** (ODbL 1.0) come under share-alike licences: a table you
  pass on with them has to stay under the same terms.
- Columns from **GeoNames** or **BIS** need the credit only.

This is a plain reading of the licences, not legal advice.

Also on the command line as `--reference` and through MCP as
`reference_data`. Both refuse while the download they need is switched off.
"#;

pub const CLASSIFY_WITH_JEV: &str = r#"# Classify with Jev

**Columns > Classify with Jev...** asks one question about every row and
adds the answer as a new column, with a second column saying how sure the
answer is. The answers come from **Jev**, a decision model by TypeSafe AI.

Jev is not a chat model and writes no text. It answers a fixed kind of
question about the cells you hand it: one of the categories you list, yes
or no, or a grade on a scale you name. Use it to sort messages by topic,
flag rows that describe a complaint, or grade free-text feedback.

## This sends your data to TypeSafe AI

The cells of the columns you choose leave your machine and go to TypeSafe
AI. Its servers are in the United States only, and it names no period after
which data is deleted. The feature is **off until you switch it on**. Send
personal or confidential data only if you are allowed to.

## Switching it on

1. Open **Settings > Classify with Jev**.
2. Tick **Allow sending data to TypeSafe AI**.
3. Paste your TypeSafe API key and click **Save key**. It goes into the
   system keyring, or into `settings.toml` when there is none. The
   `TYPESAFE_API_KEY` environment variable takes precedence.
4. Click **Refresh** beside **Model**: it lists the models your key may
   use, which also shows that the key works.

**Parallel requests** (4, from 1 to 256) is how many requests a run sends at
once. Lower it when you hit a rate limit.

## Running it

1. Tick the **columns to send**. Their cells are all the model sees of a
   row, so pick only what the question needs.
2. Choose the **answer**: **One of my categories** (2 to 255, each with an
   optional description of what belongs there), **Yes or no**, or **A grade
   on a scale** (2 to 10 levels, lowest first).
3. Type the **question** in plain words.
4. The line above the buttons says how many distinct values a run sends and
   roughly how many tokens that is.
5. Click **Send and classify**. **Stop** ends the run without changing the
   table.

## What belongs in which field

In **Settings > Classify with Jev**:

- **Allow sending data to TypeSafe AI**: a tick, once you have read the
  warning and may send this data. Everything below stays greyed without it.
- **API key**: the key from your TypeSafe account, pasted whole, then
  **Save key**. **Clear key** deletes the stored one.
- **Model**: the name of the model that answers. Leave `jev-latest` unless
  you need a fixed one. **Refresh** lists the names your key may use.
- **Parallel requests**: a whole number from 1 to 256, for example 4, or
  64 for thousands of values.

In the dialog:

- **Load from file...** and **Save to file...**: nothing to type. See
  **Saving a classifier** below.
- **Columns to send**: ticks on the columns the model has to read to
  answer, for example `subject` and `message`. Leave out everything the
  question does not need, such as ids, names or addresses.
- **Answer**: the kind of answer, **One of my categories**, **Yes or no**
  or **A grade on a scale**.
- **Question**: one question in plain words about a single row, for
  example "What is this message about?". For yes or no it has to be a
  question that yes or no answers, such as "Does this message describe a
  complaint?".
- **New column**: the name of the answer column, for example `topic`. It
  must not exist yet. Left alone it is `category`, `answer` or `score`.
  The second column takes the same name with `_confidence` or
  `_probability` behind it.
- **Categories**, the name: the short label that ends up in the cell, for
  example `billing`. 2 to 255 lines, no name twice.
- **Categories**, what belongs here: optional. A few words telling the
  model which rows belong there, for example "invoices, payments,
  refunds". Left empty, the name alone has to do.
- **Levels**: one level per line, 2 to 10, lowest first, for example
  `poor`, `fair`, `good`, `excellent`.

An example. Support messages with the columns `id`, `customer`, `subject`
and `message`, sorted by topic: tick `subject` and `message`, choose
**One of my categories**, ask "What is this message about?", name the new
column `topic`, and list `billing` (invoices, payments, refunds),
`technical` (errors, crashes), `account` (sign-in, password) and `other`
(anything else). `id` and `customer` never leave the machine. An `other`
category is worth having, or every row is pressed into a topic that does
not fit.

## What you get

Two new columns at the right, as one undo step:

- **One of my categories**: `category` (text) and `category_confidence`.
- **Yes or no**: `answer` (true or false) and `answer_probability`.
- **A grade on a scale**: `score` (a number) and `score_confidence`.

The second column is a number from 0 to 1. For yes or no it is the
probability of yes, and the answer is true from 0.5 upwards.

## Saving a classifier

**Save to file...** writes the question, its columns and its categories or
levels to a small JSON file. **Load from file...** fills the form from one.
Columns are found by name, so the file fits any table that has them, and
columns the table lacks are named under the form. Saving and loading send
nothing, and the file holds neither your data nor your key.

## Reporting a label

The labels are written by TypeSafe AI's Jev model, not by Octa. **Report** at
the bottom of the dialog (also **Help > Report AI content...**) opens a
window with a **Report to TypeSafe AI** button for a wrong or inappropriate
label. See **Chatbot**, Reporting AI content.

## Making a run faster

Raise **Parallel requests** in **Settings > Classify with Jev**, up to 256.
How many TypeSafe accepts depends on your plan. Rate-limited replies are
retried after a pause, so lower the number when that happens.

## Good to know

- Each distinct value is sent once, and every row holding it gets the
  answer.
- A row whose chosen cells are all empty is not sent and stays empty.
- A value that got no answer stays empty. The dialog then stays open with
  the count and the first error, which you can select and copy.
- Rate-limited replies are retried a few times before they count as failed.
- If the table changes while the answers are on their way, they are not
  added. Run it again.
- A partly loaded tab classifies the loaded rows only.
- Closing the dialog stops a run.

## Command line and AI agents

The same is available as `octa --classify` and as the MCP tool
`classify_rows`, which the Assistant can use too. Both obey the same switch.
Without a Settings window, put `enabled = true` under `[jev]` in
`settings.toml` or set `OCTA_JEV_ENABLED=1`, and give the key as
`TYPESAFE_API_KEY`.
"#;
