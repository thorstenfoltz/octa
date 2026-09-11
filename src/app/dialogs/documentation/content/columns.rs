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
Save and Save As both write them out. Use **Columns > Show hidden
columns** to bring everything back at once. This is a per-tab,
session-only setting; closing the tab or reopening the file clears
the hidden set.

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

Split, Merge, and Extract create new columns; Fill, Replace and Repair
garbled characters rewrite the chosen column in place. For the column-creating operations you can set the
new column name and the insert position (leave either blank for the default
shown as the field hint); for Split the name is used as a base, so the parts
become name_1, name_2, and so on. None of them change column types beyond
producing text, and all changes can be undone before you save.

## Conditional column (if / else-if / else)

**Data > Conditional column...** builds a new column whose value depends on
conditions, like a spreadsheet IF/IFS or a SQL CASE. Add an ordered list of
rules such as "if amount > 100 then high, else if amount > 50 then medium,
else low". Each rule tests one column with an operator (equals, contains,
greater than, is empty, ...) and writes its output value when it matches.

Rules are checked top to bottom and the first match wins (that is the
"else if" behaviour); reorder them with the ^ / v buttons. If no rule
matches, the Else value is used. Outputs that look like numbers become
numeric cells; everything else is text. The result is a new column (name
and position configurable) and is undoable with Ctrl+Z.

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
Yellow, Green, Blue, Purple.

The **Edit > Mark** menu, and the **Mark** keyboard shortcut (default
**Ctrl+M**), apply a single colour to the **whole current selection**: a row
block, column block, multi-cell selection, or single cell. The shortcut uses
the colour set under **Settings > Table > Default mark colour** (Yellow by
default).

Mark precedence: cell > row > column. To clear a mark, right-click and choose
**Clear Mark**.
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
- **Colour** - which of the six mark colours to paint matching cells.

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

The footer shows a live count of how many cells currently fail.

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
outlives the table it was written from. If a loaded file names a column this
table does not have, the dialog says which ones rather than dropping them
quietly: a rules file that half applies is worse than one that fails loudly.
The same file runs as `octa --check FILE --rules RULES.toml`, which exits 1 on
any violation and on any rule that could not run, and as the `check_rules` MCP
tool.
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

pub const RENAME_COLUMNS: &str = r#"# Rename Columns

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

pub const PARTITION: &str = r#"# Partition by Column

Partition by Column splits the active table into one file per distinct
value of a column, like sorting rows into folders by category. Open it via
**Data > Partition by column...** (Ctrl+Shift+Z).

## How it works

Pick the column to split on and an output folder. Octa writes one file per
distinct value (named after the value) in the format you choose. For
example, partitioning a sales table by `region` gives you `North.csv`,
`South.csv`, and so on.

The original table is not changed. Also available as
`octa --partition-by` and the `partition_table` assistant/MCP tool.

### Choosing how the pieces are named

The **Layout** option offers four shapes. Taking `city` with values `New York`,
`Berlin` and `Sao Paulo`, and writing CSV:

- **Flat files** - `new_york.csv`, `berlin.csv`, `sao_paulo.csv`
- **Folder per value** - `New York/part-0001.csv`, `Berlin/part-0002.csv`
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

Those names above are illustrations. In the dialog, **hover either option and
it shows the names your own table would produce**: the first few real values
of the column you picked, run through the same naming the writer uses, so the
tooltip and the disk agree.

Both options explain themselves on hover. Flat is the default, so nothing
changes unless you pick otherwise.
"#;

pub const OUTLIERS: &str = r#"# Detect Outliers

Detect Outliers highlights numeric values that sit far from the rest of
their column, painting each flagged cell **orange** so unusual readings
stand out. Open it via **Analyse > Detect outliers...**.

## Methods

- **IQR (interquartile range)** - flags cells outside
  `[Q1 - k*IQR, Q3 + k*IQR]`. The usual `k` is `1.5`.
- **Z-score (standard deviations)** - flags cells whose value is more than
  `k` standard deviations from the mean. The usual `k` is `3`.

Tick the columns to scan (numeric columns are pre-selected) and set `k`,
then press **Detect**. Columns with fewer than four numbers are skipped.

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
