//! Analysing and combining data: summaries, duplicates, joins, drift,
//! schema work, pivots, time series and the report generators.
//!
//! One of six topic files split out of `content.rs`, which held all 77 section
//! bodies in a single 4,261-line, 192 KB file. Text moved verbatim; the parent
//! `content/mod.rs` re-exports every constant, so `documentation::sections()`
//! is untouched.
//!
//! ASCII only: egui's bundled font renders typographic punctuation as tofu.

pub const VALUE_FREQUENCY: &str = r#"# Value Frequency

Open via the column-header right-click **Value frequency...** entry,
**Analyse -> Value frequency...** (which asks you to pick a column
first), or **Ctrl+Shift+I** (remappable; with no cell selected it opens
the same column picker). The dialog lists the most common values in one
column, ranked by count.

Each row shows:

- The distinct value (or numeric range, when binning is on).
- The count of cells matching it.
- That count as a percentage of non-null cells.

The footer reports total distinct values, total non-null cells, and
the null count. Rows are sorted by count descending; ties broken
alphabetically.

## Top-N

The toolbar offers **Top 20 / 50 / 100 / 500 / All**. The default is
**Top 50**. The choice persists per tab while the dialog stays open.
(Hidden while binning is on, since the bin count is the control there.)

## Numeric binning (histogram)

For numeric columns, a **Bin numeric values** checkbox builds a
histogram: the value range [min, max] is split into N equal-width
ranges (width = (max - min) / N) and each row counts how many values
fall in that range.

Type N into the **Bins:** field (1..1000), or leave it empty for an
automatic count via Sturges' rule (`ceil(1 + log2(n))`, clamped 5..30).

- N bins = N rows: every range is shown in ascending order, including
  empty ones (count 0), so the row count always matches what you asked.
- Labels are `[lo, hi)` half-open (last bin closed `[lo, hi]`).
- An all-identical column has no range to split, so you get one bucket.

NaN, +Inf, and -Inf show up as separate rows after the bins so type
drift is visible. Non-numeric columns hide the checkbox.

## Acting on a row

Right-click a row (when binning is off) for:

- **Copy value** - the raw value to the clipboard.
- **Filter table to this value** - adds a column filter restricting
  the active table to rows where this column equals the picked value.

The bottom **Copy as TSV** button copies the whole visible table as
`<column>\tcount\tpercent` lines.
"#;

pub const FIND_DUPLICATES: &str = r#"# Find Duplicates

Open via **Data > Find duplicates...** or **Ctrl+Shift+D** (remappable).
A modal lists every column with a checkbox - tick the ones you want
to use as the dedupe key. Two rows are duplicates when every checked
column has the same displayed text.

Output modes (radio buttons):

- **Highlight rows in place (Orange mark)**: every duplicate row in
  the active table gets an orange row mark. Use **Edit > Mark > Clear
  all marks** to remove them. Your other marks share the same path.
- **Open duplicates in a new tab**: clones the columns + just the
  duplicate rows into a fresh scratch tab. The source tab is left
  alone; the new tab has no source path so Save prompts.
- **Show only the duplicate rows**: filters the active table down to
  the repeats, without touching the data. A removable chip appears
  above the table; one click on it shows every row again.
- **Show only the rows that occur once**: the same filter inverted -
  the repeats are hidden and what is left appeared exactly once on
  the key columns.
- **Drop duplicate rows**: the one mode that edits the table. Pick
  whether to keep the **first** or the **last** occurrence of each key;
  the rest are removed as a single undoable step (Ctrl+Z brings them
  all back) and the status bar reports how many rows went. Greyed in
  read-only mode. **Ctrl+Shift+H** opens the dialog preset to this mode
  with every column ticked, so whole-row repeats are two keys away. The
  same engine runs as `octa --dedupe` and the `drop_duplicates`
  assistant/MCP tool.

Notes:

- The Apply button is greyed until at least one column is checked.
- A row whose key only matches itself is not a duplicate - results
  always come in pairs or larger groups.
- Hashing is text-based, so `Int(1)` and `Float(1.0)` render as `"1"`
  vs `"1.0"` and therefore do *not* dedupe. Change the column type
  first if you want them to.
- If no duplicates are found, the status bar reports it and the
  active table is unchanged.
- The two filter modes store the **key columns**, not the row numbers
  they resolved to, so the filter stays correct after you edit, insert
  or delete rows. They are unavailable on a very large file, where the
  tab holds one page of the file and filtering happens in SQL; hover
  the greyed radio button for the reason.
- Highlight mode clears the previous run's orange row marks before it
  paints, so a second run on different key columns does not leave the
  first run's answer behind.

The dialog seeds the key with whatever column is currently selected,
so Ctrl+Shift+D -> Apply is the fastest path for a one-column dedupe
check.
"#;

pub const FUZZY_DUPLICATES: &str = r#"# Find Near-Duplicates

**Data > Find near-duplicates...** (Ctrl+Shift+U) finds rows that are
*almost* the same on the columns you choose, not just exactly equal. It catches
typos, spacing, and reordered words (for example "Jon Smith" vs "John Smith",
or "ACME Inc" vs "ACME, Inc.") and groups the likely duplicates into clusters
with a similarity score for review.

## Controls

The dialog has **two** column choices that do different jobs:

- **Columns to compare**: the columns whose text is matched loosely (where typos
  and near-misses are found). Each candidate row pair is scored per column and
  the scores are averaged; the pair matches when the average is at or above the
  threshold.
- **Only look for duplicates within the same**: an optional column whose value
  must match exactly before two rows are even compared. Think of it as sorting
  the table into bins first, then hunting for duplicates inside each bin only.
  Example: with Columns to compare = name and "within the same" = country, the
  two US rows "Jon Smith" / "John Smith" are compared, but a German "Jon Smith"
  is never compared with the US ones. It makes large tables fast and avoids
  merging rows that clearly differ on a field you trust. Leave it empty to
  compare every row.

Other controls:

- **Method** - how two text values are scored:
  - Edit ratio: counts single-character changes. Best for typos.
  - Jaro-Winkler: rewards matching starts. Best for names and short strings.
  - Token set: compares the set of words, ignoring order and punctuation. Best
    when words are reordered ("Jon Smith" vs "Smith, Jon").
- **Similarity threshold**: how alike two rows must be (default 85%). 100% =
  identical; lower catches looser matches but risks false matches.
- **Normalise**: ignore case, collapse spaces, and ignore punctuation before
  comparing (all on by default). This is what lets "ACME, Inc." line up with
  "ACME Inc".
- **Row limit** caps how many rows are scanned (default 20,000); if the table
  is larger, the result says so.

The scan runs in the background with a **Cancel** button. Clusters are formed
transitively: if A is near B and B is near C, all three land in one cluster.
The cluster's reported score is the lowest linking similarity inside it (the
honest worst case).

## Output (tick any combination)

- **Add a cluster_id column** (default): writes a cluster_id and cluster_score
  column onto the table so you can sort or filter by cluster. One undo step.
- **Highlight** colours the near-duplicate rows orange. Re-running first clears
  the previous run's highlight, so it never builds up into a fully marked table,
  and your own marks are left alone.
- **New tab** opens a clustered report: a cluster id and score column followed
  by the original columns, grouped by cluster.

The same scan is available as the `fuzzy_duplicates` MCP / assistant tool.
"#;

pub const SUMMARY: &str = r#"# Summary

The Summary tab answers "what does this table look like?" in one click.
It is the GUI counterpart of the CLI's `octa --describe` and of pandas'
`df.describe()`: one row of statistics per column of the active table.

## Opening it

**Analyse > Summary...** opens a new tab named `Summary - <file>` for
the active table. Unsaved cell edits are included: the statistics
describe the table as you currently see it, not the file on disk.

## What it shows

One row per source column. The column headers are short, lower-case
identifiers (`column_name`, `not_null`, `total_rows`, ...) so the table
is easy to reuse elsewhere; hovering a header explains what that
statistic means in your chosen language. The available statistics are:

- **column_name** / **type** - the source column and its inferred data
  type (always shown).
- **min** / **max** - smallest and largest value.
- **sum** - total of the numeric values.
- **mean** / **median** / **std_dev** - average, middle value, and
  standard deviation (numeric columns).
- **range** - largest minus smallest value; **iqr** - the interquartile
  range (q75 minus q25).
- **q25** / **q75** - lower and upper quartiles (numeric columns).
- **mode** / **mode_count** - the most frequent value and how often it
  occurs.
- **not_null** / **null_count** / **null_percent** - counts of present
  and missing values, and the missing share.
- **unique_count** - exact count of distinct values (nulls excluded).
- **distinct_ratio** - unique values divided by total rows.
- **text_len_min** / **text_len_max** - shortest and longest text length
  in characters.
- **total_rows** - row count of the whole table.

## How Min / Max work for text

For numbers, dates, and times, **Min** and **Max** are the smallest and
largest values as you'd expect. For **text** columns the comparison is
"dictionary" order by character code, not by length or meaning:

- It compares character by character, left to right.
- It is case-sensitive, and uppercase letters come before lowercase
  ones, so `"Zebra"` sorts before `"apple"`.
- Digits compare by their character, not their numeric value, so as
  text `"10"` sorts before `"9"` (the character `"1"` comes before
  `"9"`). Numbers stored as text do not sort numerically.

If a column should sort numerically or by date, give it a numeric or
date type (Octa's date inference and the SQL view's `CAST` can help)
rather than leaving it as text.

## Choosing which statistics show

**Settings > Summary** has a checkbox per statistic. Turn off the ones
you don't care about and the Summary tab drops those columns; column_name
and type are always present. The core figures come from a single DuckDB
`SUMMARIZE` pass, plus derived null counts, an exact distinct-value
count, and (only when those statistics are switched on) one extra pass
for sum and text lengths and one per column for the mode.

## Number formatting

Numeric statistics are stored as real numbers, not text, so they follow
the same display settings as the main table and right-align like numbers.
When **thousand separators** are switched on (**Settings > Display**),
figures like sum, total rows, and the counts are grouped, and the chosen
English / European style sets the grouping and decimal marks. A numeric
column's min / max / mode group too; a text column's stay verbatim, as do
the column name and type. Saving or exporting the Summary keeps clean
numbers underneath (no separators baked in).

## Working with the result

The Summary tab is an ordinary table tab: you can sort it, filter it,
copy cells, and export it via **File > Save As**. It is a detached
snapshot with no source path, so it can never overwrite the original
file. Re-run **Analyse > Summary...** after further edits to get a
fresh snapshot.

For a deeper look at a single column, use Value Frequency instead.
"#;

pub const FILE_INTERNALS: &str = r#"# File Internals

**Analyse > File internals...** opens a read-only tab describing how the
active file is *physically written*, rather than what is in it. Summary
answers "what does this file contain"; this answers "why is it four
gigabytes and why is every query slow".

The strip above the grid carries the file-level facts: format, rows, row
groups, column count, writer version, created-by, compressed and
uncompressed bytes, bloom filters, file size.

The grid has one row per column per row group:

- **row_group / column / rows** - which chunk this is.
- **compression / encodings** - the codec and encodings for that chunk.
- **compressed_bytes / uncompressed_bytes** - what it costs on disk and
  once decoded. Sort by this to find the column eating the space.
- **nulls / min / max** - the chunk statistics. Without min/max a reader
  cannot skip row groups, so every query reads everything.

Up to three plain-language hints appear when the layout looks poor: very
small row groups, missing column statistics, or no compression. All
three are fixable from Octa via the write options in Save As and Batch
convert.

Parquet reports full detail. Other formats report their size and say
they have no inspectable internal structure. A tab with no file behind
it has nothing to inspect.

The same information is available as `octa --describe FILE --deep` and,
over MCP, as `describe_file` with `deep: true`.
"#;

pub const FUZZY_JOIN: &str = r#"# Fuzzy Join

Two tables describe the same customers, and neither shares a key with the
other. The CRM says `Mueller GmbH`, the sales sheet says `Mueller Gmbh.`, and
Join tables matches neither of them. Fuzzy join matches rows that are
**similar** rather than identical.

**Opening it.** **Data > Fuzzy join...**, with at least two tables open. No
default keyboard shortcut; assign one under **Settings > Shortcuts** if you
want it.

**Setting it up.** Pick the left table, whose rows are kept, and the right
table, which is searched for a partner. Then say which columns to compare:
one pair, or several when a single column is not distinctive enough, in which
case their scores are averaged.

- **Edit ratio** suits typos and small misspellings.
- **Jaro-Winkler** suits names, where a shared beginning counts for more.
- **Token set** ignores word order and punctuation.

These are the same measures Find near-duplicates uses within a single table.
Values are compared as normalised text: lowercased, spaces collapsed,
punctuation dropped, which is what lets `  ACME  Ltd. ` meet `acme ltd`.

**The threshold** is how similar two values must be to count as a match, from
0 to 1. 0.85 is a sensible start: raise it when you get matches you do not
believe, lower it when obvious pairs are missed.

**Blocking.** Without a blocking column every left row is compared with every
right row, which grows with the product of the two row counts and is why a row
cap exists. Naming a column that must match exactly, such as a country or a
postcode, restricts the comparison to rows that already agree there. It
changes which pairs are compared, never which of them match.

**Reading the result.** The result opens in a new tab with the left columns,
the right columns, and two more per step. `match_score_N` is how similar the
matched pair was, empty when the row found no partner. `ambiguous_N` is true
when the runner-up scored nearly as well: those are the rows to check by hand,
because the join picked one but it was a close call. The status bar reports
how many rows matched, how many are ambiguous, and whether the row cap was
reached.

**More than two tables.** **Add another table** joins the result to a third,
folding left to right. Each step gets its own score and flag rather than one
number for the whole chain, because matching against an already fuzzy result
compounds the error and a single number would hide where it came from.

**Ceilings.** Each left row keeps one partner, the best scorer, so a row that
genuinely matches two right rows is not expressible. Values are compared as
text, with no numeric or date tolerance. The row cap applies per side and is
only reached without a blocking column. There is no accept/reject review of
individual matches yet: use `ambiguous_N` to find the ones worth a look.

**Elsewhere.** The same engine runs as `octa --fuzzy-join` and as the
`fuzzy_join` MCP tool, which the Assistant can call too.
"#;

pub const REPORT: &str = r#"# Report

A report turns the table you are looking at into one HTML file you can send to
somebody: per-column statistics, a chart for each column, the most common
values and a correlation matrix.

The file is self-contained. Its styling is inline, its charts are inline SVG,
it runs no JavaScript and it fetches nothing, so it opens from a mail
attachment on a machine with no internet.

**Opening it.** **File > Report...**. No default keyboard shortcut; assign
one under **Settings > Shortcuts** if you want it. The report covers the
active tab, follows whatever filter is applied to it, and includes your
unsaved edits, exactly as the Summary tab does.

**What goes in.** Each section can be switched off:

- **Column statistics**: type, nulls, unique values and the numeric summary
  for every column.
- **Distribution charts**: a histogram per numeric column, a bar chart of the
  commonest values otherwise.
- **Most common values**: the most frequent values per column, with counts and
  shares.
- **Correlation**: how the numeric columns move together.

Nothing here is calculated a second time. The statistics come from the same
engine as the Summary tab, the top values from Value frequency, the matrix
from Correlation, and the pictures from the Chart tab's own SVG export, so the
report cannot disagree with what Octa shows on screen.

Distributions draws at most 50 columns and then says how many it left out.
Correlation needs at least two numeric columns; below that the section is left
out rather than showing a column's correlation with itself.

**Profiling a sample.** **Profile a sample only** is off, so every row is
examined. Turn it on for a faster, approximate report on a very large table.
The document then states how many rows it looked at and how many there were,
so nobody mistakes approximate numbers for exact ones. Under an active filter
the sample is drawn from the rows the filter leaves visible.

**When it is done.** Building runs in the background, so Octa stays usable,
and **Cancel** stops it. When it finishes the dialog shows where the file went
and offers **Open in browser**.

**Elsewhere.** The same engine runs as `octa --report OUT.html FILE` (with
`--report-sections` and `--report-sample`) and as the `create_report` MCP
tool, which the Assistant can call too.
"#;

pub const HARMONISE: &str = r#"# Harmonise Schemas

The write half of Schema drift. That scan tells you 497 parts look like this
and 3 look like that; this rewrites the odd ones out.

Open it from **File -> Harmonise schemas...**, or press **Harmonise...** in
the Schema drift dialog, which carries the folder and options across.

## How it works

Two steps on purpose.

**Plan** scans the folder and shows what would happen without writing
anything: how many files change, how many already match, how many are
refused, the target columns, and, crucially, **which columns get dropped**.

**Harmonise** then writes.

The split exists because dropping a column is the only lossy part of the
operation, and you should see that before committing rather than read about
it in the report afterwards.

## What it does to each file

- A column missing from a file is **added, filled with nulls**.
- A column not in the target is **dropped**, and named in the plan and the
  report.
- A column whose type differs is **cast**.
- Columns are reordered to the target order.

The target is the shape most files in the folder already have, since that
needs the fewest rewrites.

## Two safety properties

**Your files are never modified.** Harmonised copies go to a separate output
folder, which is required rather than defaulted. If the result is wrong, you
have lost disk space and nothing else.

**A file that will not cast is refused, not emptied.** If a column holds
`not-a-number` and the target wants a whole number, that file is skipped with
a reason instead of written with blanks where the values were. A harmonised
folder full of silently emptied cells looks clean and is not, which is the
worst outcome this feature could have.

Two input files from different subfolders that share a name would write to the
same output. Both are refused rather than one being quietly renamed.

## Elsewhere

Also available as `octa --harmonise-schema DIR --out-dir DIR` (exits 1 if any
file was refused, so CI can gate on it) and the `harmonise_schemas` tool for
the Assistant and MCP.
"#;

pub const SCHEMA_DRIFT: &str = r#"# Schema Drift

A folder of data files is supposed to be one table. Schema drift finds the
files where it is not: the part written with `amount` as text, the one that
lost a column, the one whose header is `Amount` rather than `amount`.

Nothing is read but the columns. Parquet reads its footer and Arrow IPC its
header, so scanning hundreds of files costs very little.

**Opening it.** **File > Schema drift...**, then pick the folder. Or
right-click a folder in the sidebar and choose **Scan schemas...**, which
opens the same dialog with that folder already filled in. No default keyboard
shortcut; assign one under **Settings > Shortcuts** if you want it.

**Options.** **Include subfolders** (off by default) walks subfolders too, to
a depth of 8, which is what `year=2024/month=03` layouts need. **Ignore upper
and lower case** (off by default) treats `Amount` and `amount` as one column;
case is compared exactly otherwise, because to some downstream tools a
renamed-only-in-case column really is a different column.

**Reading the result.** The scan opens a **Schema drift** tab and the status
bar summarises it in a sentence. Files are grouped, not listed: every file
with an identical schema collapses into one variant, and the variants are
ordered largest first, so the odd file out is visibly the minority. For 500
Parquet parts the answer is "497 look like this, 3 look like that" rather than
500 rows.

The table has a `status` column, a `column` column, then one column per
variant holding that variant's type for that column, or blank where the
variant has no such column. Rows that need attention sort to the top:
`type varies` (every variant has it, with differing types), `missing in N`
(N variants lack it entirely), then `consistent`.

A file that cannot be read does not stop the scan: it is counted in the status
line as skipped, and the rest are still compared.

**Ceilings.** Local folders only, no cloud prefixes. Multi-table sources (a
workbook, a database file) report their first table. Include subfolders stops
at depth 8.

**Elsewhere.** The same engine runs as `octa --schema-drift DIR`, which exits
1 when the files disagree so a CI step can gate on it, and as the
`schema_drift` MCP tool, which the Assistant can call too.
"#;

pub const DATA_DRIFT: &str = r#"# Data Drift

Two versions of the same data, side by side, answering one question: does
today's extract still look like yesterday's? Not which rows changed, which is
what Compare and `--diff` answer, but whether the shape held: the same columns,
the same fill rate, the same range, the same categories.

**Opening it.** **Analyse > Data drift...**. Pick a source on each side, an
open tab or a file from disk, put the older version in **Before**, and press
**Compare**. The result opens in its own tab. No default keyboard shortcut;
assign one under **Settings > Shortcuts** if you want it.

**Reading the result.** One row per column per metric, in six columns.
Hovering a header in the result tab shows the same explanation.

- **column** - which column the row is about; blank for table-wide rows such
  as the row count.
- **metric** - what is measured. `rows` is the table's row count.
  `null_rate` is the share of empty cells in the column, 0 to 1.
  `distinct_count` is how many different values it holds. `min` / `max` /
  `mean` are over the numeric values. `new_values` / `vanished_values` list
  categories that appeared or disappeared, only for columns under the
  category limit.
- **before** / **after** - the value on each side.
- **change** - how far it moved *relative to before*: (after - before) /
  before. 0.1 means it moved by a tenth, whether that is 5 rows out of 50 or
  5 million out of 50 million, which is what makes one threshold usable
  across columns of very different size. Blank where a relative change is
  meaningless, such as a list of categories. Negative when the value fell.
  **A baseline of 0 that moved at all counts as an unbounded change**, so it
  breaches any threshold - a null rate going from 0 to 0.5 is infinitely
  worse in relative terms, and reporting 0 there would hide the most
  interesting kind of drift.
- **breached** - whether the row crossed a threshold. False everywhere unless
  you set thresholds with `--fail-on`; the dialog sets none, so a comparison
  run from the window reports movement without judging it.

**What it compares.** Columns are matched by name. A column on one side only is
reported as added or removed and gets no other rows: there is nothing to
compare it against. Every shared column reports its null rate and distinct
count, plus minimum, maximum and mean when both sides are numeric. Columns with
few enough distinct values also list the category values that appeared and
vanished by name, capped by the **Category limit** field (50 by default),
because listing the new entries of a free-text column is noise rather than
drift.

Every number comes from the same Summary pass the Summary tab uses, called once
per side, so the two can never tell you different things about one file.

**Ceilings.** Columns are matched by name only, so a renamed column reads as
one removed and one added. Both sides are read under the usual row cap.
Category lists name at most ten values, then say how many more there were.

**Elsewhere.** `octa --drift-report A B` prints the same report, and
`--fail-on null_rate:0.05,rows:0.1` turns it into a CI gate that exits 1 when a
metric moved too far. The `data_drift` MCP tool answers the same question with
a `failed` flag, and the Assistant can call it.
"#;

pub const REL_MAP: &str = r#"# Relationship Map

How do these tables connect? One box per table listing its columns, a line
between each pair of columns that relate, and a plain sentence on every line
saying how well they match.

**Opening it.** **Analyse > Relationship map...**. Choose the tabs you have
open, or a folder of data files, and press **Scan**. Reading values takes a
moment, so it runs in the background with a Cancel button. Drag the boxes into
whatever arrangement suits you, and **drag a score chip to bend its line**: the
chip is the curve's handle and the whole connection follows it, so two lines
running through the same space can be pulled apart instead of overlapping. A
chip you have not touched leaves its line straight. No default keyboard
shortcut; assign one under **Settings > Shortcuts** if you want it.

**What decides a line.** The same ranking as the Join key finder, whose help
section sets the arithmetic out in full with a worked example. Column names
take no part in it, only values. In short, each column becomes the set of its
distinct values, read from a sample of rows, trimmed to text, empty cells
skipped, and then for every pair of columns across two tables:

    shared       = how many values appear in BOTH distinct sets
    overlap      = shared / the smaller of the two distinct counts
    distinctness = distinct values / non-empty values sampled  (each side)

    score        = overlap * the LARGER of the two distinctness values

    orphans      = distinct values on the left - shared

Overlap is 1.00 when every value of the smaller side also exists on the larger
side. Distinctness is how close a column comes to a different value in every
row: a primary key is 1.00, a status column with three values across ten
thousand rows is 0.0003. Taking the larger of the two is deliberate, since a
foreign key is unique on the parent side and repeats on the child side, so
asking whether *either* side identifies a row is the question with a yes for
every real key. Multiplying is what stops a status column that happens to
overlap perfectly from outranking a real key - and it is the **only** thing
separating them, because a status pair usually has a perfect overlap and no
orphans either.

**How much is enough.** A line is drawn once its score reaches the threshold in
**Show links from**, under the source options, which starts at **0.50**. So a
pair whose values overlap completely needs the more distinct side to be at
least half distinct; a pair overlapping only half the time needs a side that is
essentially unique. Move it down to find weaker or partial links - a foreign
key only half the rows use sits well below 0.50 - and up to keep only the
strongest. The box beside it takes any value between 0 and 1 typed exactly,
with either . or , as the decimal mark: the slider is for sweeping, the box for
pinning a number down. It applies on the **next Scan**, since the threshold is
used while the values are being compared.

Each line also carries an orphan count: how many distinct values on one side
find no partner on the other. That is the number meant to separate two
candidates which score identically, and they do exactly when both tables
number their rows from 1: with 1,000 orders and 4 customers, both
`orders.id -> customers.id` and `orders.customer_id -> customers.id` score a
perfect 1.00, and only the first leaves 996 orphans.

An orphan count is directional, and the two ways round answer different
questions ("customers who never ordered" against "orders pointing at a
customer who is gone"). Only one of them settles a tie, and nothing in the
arithmetic knows which of your tables is the child, so **both counts sit on
every line** - hovering shows one sentence per direction. In the example above
the customers side reads 0 for both candidates while the orders side reads 996
and 0. The Join key finder section works it through in full. A declared foreign
key has no ambiguity to begin with: it knows which end is the child, and
Measure scores it in that direction.

The orphan count is computed from the same sampled sets the score came from,
so the two numbers on one line can never contradict each other.

**A live database needs no guessing.** The two sources above read values and
infer. A **Database** source does not have to: somebody already declared the
foreign keys, and the server hands them over for the asking. Pick a saved
connection, tick the schemas, and Scan. That reads **catalog information only,
no table data**, so the size of the tables does not matter. Every line is a
declared foreign key and names its constraint in the tooltip.

Nothing was measured, so the chips read **FK** instead of a score. A
declaration and a fact are not the same thing: Postgres, MySQL, SQL Server,
Oracle and Exasol enforce their foreign keys, so a line from those servers is
true of the rows as well, while Redshift, Snowflake, Databricks and BigQuery accept a
declaration and enforce nothing. **Measure** answers that: it reads a sample of
rows from each drawn table and fills in the same overlap, score and orphan
counts, so a declared key nothing honours shows up as orphans. It is a separate
button and never automatic, because it is the step that reads your data.

The **Tables** list under the schemas holds every table the scan saw. The ones
taking part in a foreign key start ticked and the rest do not, since a grid of
boxes with no lines between them is an inventory rather than a map. Ticking one
redraws at once, without asking the server again. ClickHouse has no referential
constraints of any kind, so there is nothing to read there.

**Boxes that hold more than they show.** A box lists the first twelve columns
and then a `+N` row saying how many it left out. That row is a button: click it
and the box lists every column, click the `-N` it becomes and it folds back.
It is not only cosmetic, because a line attaches to the row of the column it
names and a column past the twelfth has nowhere to attach while the box is
folded, so it lands on the last visible row. Open the box and the line moves to
the column it is really about. Dragging the box still works from that row.

**Exporting.** The **Export...** button writes the map as it stands at that
moment: boxes where you dragged them, lines bent the way you bent them, column
lists open as far as you opened them. The picker beside it chooses the format
and remembers the choice, so "always SVG" is set once. **PDF** (the default)
and **SVG** are vector and stay sharp at any size, **PNG** is rendered at 2x
for pasting into a slide or a chat, and **HTML** is a self-contained page for
someone who does not have Octa: it fetches nothing, works offline, and still
pans, zooms and lets the boxes be dragged anywhere on screen with the lines
re-routing live.
Bending a line, clicking through to Join and Measure need Octa. Nothing is a
screenshot: all four go through one hand-emitted SVG, so the export does not
depend on your window size, your zoom or your screen's DPI.

**Using a line.** Click one and the Join dialog opens with that pair already
filled in. That works for open tabs; a folder or database scan draws the map
and leaves the joining to you, since the tables are not open.

**Ceilings.** Sampled at 10,000 rows per table, so a high score is strong
evidence rather than proof. Single columns only. Values are compared as trimmed
text. A folder scan reads at most 30 files and a database scan at most 30
tables, and both say when they stopped. A declared key whose other end is not
drawn is counted and reported rather than drawn into nowhere.

**Elsewhere.** `octa --relationships DIR` prints the same ranking with its
orphan counts, and `suggest_join_keys` carries them to the Assistant.
"#;

pub const JOIN_DIAG: &str = r#"# Join Diagnostics

You expected 10,000 matched rows and got 12. This tells you why, opened from
**Analyse -> Join diagnostics...**

Pick a table and key column on each side and press **Diagnose**. It reports:

- **Rows read** and **Distinct keys** per side. Counts are over distinct key
  values, not rows: a join failing on three IDs is one problem however many
  rows carry them.
- **Matching keys**: how many distinct keys exist on both sides right now.
- **What would help**: the single normalisation that would raise that number,
  such as trimming spaces, ignoring case, or ignoring leading zeros. A fix is
  listed only when it **strictly beats** the current count, so an empty list is
  a real answer: no easy change helps, and the columns probably hold genuinely
  different things.
- **Only on the left / right**: a few real unmatched values from each side, so
  you can see what you are dealing with.

It **changes nothing**. The fixes are advice; act on them with Transform column
or by fixing the source. **Use in Join** hands the two columns to the Join
dialog once you are satisfied.

**No language model is involved.** Each suggested fix is the same matching-key
count recomputed with one normalisation applied, run locally, same answer every
time.

Sampled at 10,000 rows per side by default. When either table is longer the
report says so, because the counts are then partial.
"#;

pub const JOIN_KEYS: &str = r#"# Join Key Finder

**Analyse > Join key finder...** ranks the column pairs that would
actually join the tables you have open, by looking at the values rather
than the names. It answers "which columns do I join on?" before you open
the Join dialog on two unfamiliar tables.

For every column pair across every ticked table pair it measures:

- **Overlap**: how much of the smaller set of distinct values appears in
  the larger one. A real key pairing is near 100%.
- **Distinct**: distinct values relative to rows sampled. A key is near
  100%; a status or flag column is near zero.

## How the score is calculated

**Step 1, each column becomes a set.** Octa reads the first 10,000 rows
(the Sample rows per table box) and keeps that column's distinct values,
plus how many non-empty values it saw. Cells are turned into text and
trimmed, and empty cells are skipped entirely, counting towards neither
number. So `1` in an integer column equals `"1"` in a text column, `1`
and `1.0` do not, and a column that is half nulls is judged on the half
that is there.

**Step 2, every pair of columns is scored:**

    shared       = how many values appear in BOTH distinct sets
    overlap      = shared / the smaller of the two distinct counts
    distinctness = distinct values / non-empty values sampled  (each side)

    score        = overlap * the LARGER of the two distinctness values

    orphans      = distinct values on this side - shared   (counted BOTH ways)

A pair sharing no value at all is not a candidate, and a pair scoring
below 0.2 is dropped as noise. The Relationship map raises that floor to
0.50 by default. Every factor sits between 0 and 1, so the score does too.

**Why the larger distinctness and not the smaller.** A real foreign key is
unique on one side only: `customers.id` has a different value in every
row while `orders.cust_id` repeats it once per order, so the child side's
distinctness is low by design. Taking the smaller would punish exactly the
shape being looked for. Taking the larger asks "is at least one of these
two columns something that identifies a row?", which is true of every key
pairing and false of a status column on both sides.

**Worked example.** Two files, small enough to check by hand:

    customers.csv            orders.csv
    id,name,status           order_id,cust_id,status
    c1,Alice,open            1,c1,open
    c2,Bob,shut              2,c1,shut
    c3,Cara,open             3,c2,open
    c9,Dan,open              4,c3,open
                             5,c3,open

For customers.id against orders.cust_id:

    distinct left    c1 c2 c3 c9                        4
    distinct right   c1 c2 c3                           3
    shared           c1 c2 c3                           3
    overlap          3 / min(4, 3)                      1.00
    distinctness L   4 distinct / 4 non-empty values    1.00
    distinctness R   3 distinct / 5 non-empty values    0.60
    score            1.00 * max(1.00, 0.60)             1.00
    orphans          4 - 3                              1

The orphan is c9, the customer who never ordered: a fact about the data,
not a fault in the pairing. Now the trap, the two status columns, both
holding just `open` and `shut`. They overlap **perfectly** and leave
**zero** orphans:

    overlap          2 / min(2, 2)                      1.00
    distinctness L   2 distinct / 4 non-empty values    0.50
    distinctness R   2 distinct / 5 non-empty values    0.40
    score            1.00 * max(0.50, 0.40)             0.50

Half the score of the real key, with nothing but distinctness separating
them. At realistic sizes it collapses further: 2,000 customers and 10,000
orders with three statuses score 1.00 * 3/2000 = 0.0015, under the floor,
so it is not reported at all.

**Orphans break the ties.** Two candidates can score identically and only
one be real. It happens whenever both tables number their rows from 1,
which is most tables with an auto-increment key. Take `customers.csv`
with ids 1 to 4, and `orders.csv` with 1,000 orders, its own id 1 to
1000, and a customer_id pointing at one of the four:

    customers.id vs orders.customer_id     the real foreign key
    customers.id vs orders.id              a coincidence, orders 1-4 exist

Both score a perfect 1.00: full overlap, and a completely distinct side.
The orphan counts break that tie, and they are reported **both ways
round**, because only one of the two directions can settle it and nothing
in the arithmetic knows which of your tables is the child:

    pairing                              customers side   orders side
    customers.id vs orders.id            0 of 4           996 of 1000
    customers.id vs orders.customer_id   0 of 4           0 of 4

Read from customers the two are identical, since all four ids appear in
both columns they are compared against. Read from orders, 996 of the
1,000 order numbers point at no customer at all while every customer_id
finds one. That settles it, and it no longer matters which table you
opened first. A relationship map from a live database has no ambiguity in
the first place: a declared foreign key knows which end is the child, and
Measure scores it in that direction.

**Which side is left.** Overlap and score are symmetric, so swapping the
two columns gives the same number. Orphans are not, which is why both
directions are shown; left is simply the table that came first, not a
claim about which one is the parent.

**No language model is involved.** This is set arithmetic over the
sampled values, computed on your own machine: nothing is sent anywhere,
and the same tables always produce the same ranking.

Tick two or more tables (three gives every pairing between them), adjust
the sample size if you like, then press Scan. **Use in Join** opens the
ordinary Join dialog with that pair filled in, so there is still only one
join implementation.

Ceilings: sampled, so a high overlap is evidence rather than proof;
single columns only, no composite keys; values compare as trimmed text,
so a numeric and a text column holding the same ids still pair up.

Over MCP the same ranking is `suggest_join_keys`.
"#;

pub const PIVOT: &str = r#"# Pivot / Unpivot

Reshape a table between **long** and **wide** form, the way a spreadsheet
pivot table does. Open it via **Analyse > Pivot / Unpivot...**. The result
always opens in a **new detached tab** - your original table is never
changed. It runs on the table as you currently see it, including unsaved
edits.

## Pivot (long to wide)

Pivot spreads one column's distinct values into new columns. Pick:

- **Spread column** - the column whose values become the new column
  headers (e.g. `month`, producing one column per month).
- **Aggregate** - how to combine the values that fall into each new cell:
  `sum`, `count`, `avg`, `min`, or `max`.
- **of** - the value column being aggregated (e.g. `sales`).
- **Group by** - the identity columns kept as rows (e.g. `region`). Leave
  this empty to let DuckDB use every remaining column.

Example: spread `month`, aggregate `sum` of `sales`, group by `region`
turns a long sales log into a region-by-month grid of totals.

## Unpivot (wide to long)

Unpivot is the reverse: it melts several columns into two columns, a name
and a value. Pick the **columns to unpivot** (at least two), then name the
generated **name column** and **value column**. A wide `region, jan, feb,
mar` table becomes a long `region, name, value` table with one row per
region-month.

## Live preview

While the dialog is open it shows a plain-language sentence of what the
current settings do, plus a small preview table of the first result rows.
To stay fast on big tables the preview runs on a sample of the first 1,000
source rows and shows up to 10 result rows; press **Run** to reshape the
full table.

Powered by DuckDB's `PIVOT` / `UNPIVOT`, so it works on any open table.
"#;

pub const CORRELATION: &str = r#"# Correlation

Measure how strongly the numeric columns in a table move together. Open it
via **Analyse > Correlation...**, pick a method, and press **Compute**. The
result opens in a **new detached tab** - your original table is unchanged.

## Methods

- **Pearson** measures linear association (do the values rise and fall
  together in a straight-line way).
- **Spearman** measures monotonic association by correlating the value
  ranks, so it catches consistent up-or-down relationships that are not
  perfectly straight.

## Reading the result

Every numeric column is correlated with every other numeric column. The
result is a square table: the first column lists each variable, and there
is one further column per variable. Each cell holds a coefficient from
**-1** (perfectly opposite) through **0** (no linear/monotonic relation) to
**+1** (perfectly together); the diagonal is always 1. A pair with too few
overlapping values, or no variation, is left blank. Non-numeric columns are
ignored automatically.
"#;

pub const SCHEMA_EXPORT: &str = r#"# Schema Export

Open via **File > Export schema...** or **F7** (remappable).
The dialog opens on the first target (Postgres DDL); switch between
the ten supported targets with the chip row at the top of the
dialog.

Supported targets:

- **SQL DDL (Postgres)**: CREATE TABLE with double-quoted identifiers.
- **SQL DDL (MySQL)**: CREATE TABLE with backtick identifiers + UNSIGNED / DATETIME / BLOB types.
- **SQL DDL (SQLite)**: CREATE TABLE with INTEGER / REAL / TEXT / BLOB affinity.
- **SQL DDL (MS SQL Server)**: CREATE TABLE with bracket identifiers + NVARCHAR(MAX) / BIT / DATETIME2 types.
- **SQL DDL (Databricks)**: CREATE TABLE with Spark SQL / Delta types (STRING, TIMESTAMP_NTZ).
- **SQL DDL (Snowflake)**: CREATE TABLE with Snowflake types (VARCHAR, TIMESTAMP_NTZ).
- **Pydantic v2**: BaseModel subclass with date / datetime imports.
- **TypeScript interface**: number / string / boolean mappings.
- **JSON Schema** (draft 2020-12): object schema with properties + required.
- **Rust struct**: serde-derived struct with chrono types.

Buttons in the footer:

- **Copy to clipboard**: puts the rendered text on the clipboard.
- **Save as...**: opens a save dialog pre-filled with
  `<source_name>_schema.<ext>`.

Type mapping:

- Octa stores types as Arrow strings ("Int64", "Utf8", "Float64",
  "Date32", "Timestamp(...)", ...). Each target maps them to its
  closest native type.
- Unknown Arrow types fall back to each target's TEXT-equivalent
  with a comment so the output is never silently wrong.
- A timestamp maps to the target's zoned column only when the column
  type actually **names a zone**. Readers spell a naive timestamp three
  ways - `Timestamp(us)` (Parquet, Arrow IPC), plain `Timestamp` (ORC)
  and `Timestamp(Microsecond, None)` (DuckDB and the text formats) -
  and all three export to the tz-less column. The same mapping produces
  the CREATE TABLE behind database write-back.

Identifier safety:

- Column names with spaces / hyphens / leading digits get quoted
  (SQL, TypeScript) or sanitised + aliased (Pydantic Field(...,
  alias=...), Rust #[serde(rename = "...")]) so the model still
  round-trips JSON / CSV with the original key.

The active row filter does *not* affect schema export -- only the
column list does.
"#;

pub const DATA_QUALITY: &str = r#"# Data Quality Report

**Analyse > Data quality report...** opens a new tab that scores each column of
the active table, so you can see at a glance where the data needs cleaning.

## What it shows

One row per source column, with these columns (hover any header for a full
explanation):

- **null_percentage** - percentage of missing values.
- **distinct_ratio** - distinct values divided by non-null values (1.0 means
  every value is unique).
- **outlier_count** - number of numeric outliers, using the same IQR method as
  Detect outliers.
- **pii_flag** / **pii_kind** - whether the column looks like personal data,
  and what kind, reusing Detect PII.
- **type_consistency** - the share of values that actually match the column's
  declared type.
- **score** - this column's mark out of 100, worked out from three of the
  columns above it: 40% for how much of it is filled in (null_percentage), 40%
  for how much of it matches the declared type (type_consistency), and 20% for
  how varied the values are (distinct_ratio). Numeric outliers take up to 10
  points off. It says nothing about whether the values are correct, only about
  how well formed they are.

The overall table score is the mean of the column marks. It sits in the tab
title ("Quality 81/100 - sales.parquet") so it stays on screen, and in the
status bar when the report opens. Hovering the tab repeats the explanation;
hovering the score column header explains a single column's mark.

The report is an ordinary table tab: sort it, filter it, or save it like any
other file. Re-run the report after cleaning to see the score improve.

## Hover a verdict to see what it means

The two verdict columns below hold labels rather than data, and a label short
enough for a cell cannot also say what it means. Hovering one explains it: what
the verdict is telling you, and what it is not.

That matters most for the values that are not verdicts at all. Every one of
them starts **not tested:** and names the gate it tripped, and the hover says
why that gate exists. **not tested: narrow range** is not a complaint about the
column; it is Octa declining to judge a percentage by a law percentages have no
reason to follow.

The column header keeps its own tooltip describing the column as a whole, so
the two answer different questions: what is measured here, and what does this
answer mean.

## Benford's law

The **benford_verdict** column asks whether a numeric column's leading digits
look like measured quantities. Numbers that arise from measuring or
accumulating things across several orders of magnitude start with a 1 about
30% of the time and a 9 under 5%; invented or capped figures usually do not.

A verdict is one of **conforms**, **acceptable**, **marginal** or
**nonconforming**, in rising order of "worth a second look".

**Most columns get no verdict at all, and that is the point.** A column that
had no reason to follow the law would read as nonconforming and teach you to
ignore the column, so four gates come first and the cell says which one it
tripped:

- **not tested: not numbers** - the column is not numbers.
- **not tested: too few values** - fewer than 300 usable values, so the digit
  shares are noise.
- **not tested: narrow range** - values span less than a factor of ten. A
  percentage, an age or a rating is a bounded range and has no reason to follow
  the law.
- **not tested: looks like ids** - dense, distinct whole numbers: a row number,
  an invoice number, an id. Those are assigned, not measured. The gate tests
  density, not uniqueness, so Fibonacci numbers - whole, all different, and
  spread across their range - are still tested.

A verdict is evidence to look further, never a finding on its own.

## Calendar coverage

The **calendar_verdict** column walks a time column's calendar. A daily series
with four days missing in March looks healthy in every other statistic here,
and the only way to see it is to check the dates one by one. Octa infers the
step from the most common distance between timestamps, then looks for holes:
**complete**, **weekdays only**, **complete (clock change)** or **gaps**.

**Two false alarms are ruled out rather than reported**, because either one
would make the check useless:

- **A weekday-only series is not broken.** Business data skips Saturdays and
  Sundays on purpose. Both ends have to line up, so a Friday-to-Monday hole is
  a weekend and a Friday-to-Wednesday hole is still missing data.
- **A daylight-saving change is not missing data.** Octa's timestamps carry no
  timezone, so a spring-forward reads as a one-hour hole. The shape is used
  instead: one step missing, on a Sunday, in the small hours, in a series finer
  than a day. It gets its own verdict because it is a guess; the same hole on a
  Wednesday afternoon is reported as missing.

Columns that are not a series say why: **not tested: not dates**,
**not tested: too few points** (fewer than three distinct timestamps) or
**not tested: no regular step** (no step accounts for 60% of the intervals).

The gaps themselves open in a **Calendar gaps** tab, one row each, with the
timestamps either side and how many steps are absent. Weekends are not listed -
a five-year weekday series has 260 of them and `weekdays only` is the whole
finding.

## Extra tabs for findings that are not per column

Some problems do not fit one row per column, so they open in their own tab
beside the report. Focus stays on the main tab, and the status bar says how
many extra tabs appeared. A finding with nothing to report opens no tab at
all, so a clean file still gives you exactly one.

An extra tab arrives without your having asked for it by name, so it
introduces itself: hover the tab for a one-sentence answer to "what am I
looking at?", and hover any column header in it for what that column holds.

**Missing together** is the first of them. "This column is 12% null" is
already on the main table and does not tell you much: four columns each 8%
null are a different problem depending on whether they are empty in the *same*
rows (one upstream join that did not match) or in different ones (four
unrelated gaps). This tab lists the sets of columns that go missing together,
how many rows share each set, and what share of the table that is, biggest
first.

Three rules keep the list short enough to read:

- **A column that is null on its own is not a pattern.** That is the null
  percentage the main table already gives you, and repeating it here would
  bury the multi-column findings this tab exists for.
- **A handful of rows is not a pattern either.** A set has to cover at least
  1% of the table and at least 5 rows, so a small file does not report noise.
- **An empty text cell counts as missing** alongside a real null. It is a hole
  in the data whatever the file called it, and readers differ on which one a
  blank field becomes.
"#;

pub const DIST_COMPARE: &str = r#"# Compare Distributions

**Analyse > Compare distributions...** answers one question: do these two
columns look like the same population?

It is the question behind "did something change" - last month against this
month, control against treatment, June's file against July's. A mean and a
standard deviation answer it badly: two samples can share both and still be
shaped nothing alike.

## Picking the columns

Two rows, each a tab picker and a column picker. Both start on the tab you
opened the dialog from, because comparing two columns of one table is as common
as comparing one column across two files. Changing a tab clears the column
beside it: index 3 of one table is not index 3 of another.

**There is no test picker.** The engine chooses from what the columns hold:

- **Numbers** use a two-sample **Kolmogorov-Smirnov** test, which compares the
  whole shape rather than one summary of it.
- **Anything else** is compared as categories with a **chi-square test of
  homogeneity**.

A column that is not numeric throughout is compared as categories, which is the
honest reading of a column that is not really numeric.

## Reading the answer

The result opens as its own tab, and the headline also lands in the status bar:

- **headline** - a sentence, such as "The second sample skews 12% higher." For
  categories it names the category whose share moved most.
- **verdict** - `same` or `different`, at the conventional p = 0.05.
- **test**, **statistic**, **p_value** - the evidence, underneath the answer.

`D = 0.184, p = 0.003` tells almost nobody anything, which is why the sentence
comes first and the number stays for whoever wants it.

## When it does not apply

Instead of a verdict you get a reason: **na_too_few_values** (fewer than 20
usable values in a column), **na_too_many_categories** (more than 50 distinct
values, so it is free text) or **na_too_few_categories**.

Two details about the categorical test: rare categories are **pooled into one
(other) bucket rather than dropped**, so their rows still count towards the
totals; and nulls are left out entirely, because a column getting emptier is a
completeness finding and the quality report's null_percentage is where that
lives.
"#;

pub const REFERENTIAL: &str = r#"# Referential Integrity

**Analyse > Referential integrity...** answers one question: which child rows
point at a parent that is not there?

A join that silently drops rows is the most expensive kind of wrong, because
the result still looks like a table. This names the values responsible before
you run it.

## Picking the columns

Two rows, each a tab picker and a column picker: the **parent key** and the
**child key** that points at it. Both start on the tab you opened the dialog
from, so a self-reference - a manager_id pointing at id in the same table -
needs no extra clicks. Changing a tab clears the column beside it, because
index 3 of one table is not index 3 of another.

## Reading the answer

**A clean check opens no tab.** There would be nothing to look at, and the
status bar saying so is the whole answer.

When there are orphans they open as their own tab, one row per offending value,
biggest first: the child column, the value with no parent, and how many rows
carry it. The counts ride the tab's notice banner. The list stops at 500
distinct values; the counts stay exact however many there are.

## Two conventions worth knowing

- **A missing key is not an orphan.** In every relational database a null
  foreign key means "no parent", not "a parent that vanished", so counting
  those would report every optional relationship as broken. They are counted
  and reported separately. An empty text cell counts as missing too, because
  that is how a CSV writes an absent value.
- **Keys are compared as trimmed text**, the same convention the relationship
  map and the join key finder use. That is what makes 1 match 1 when one side
  came from a CSV and the other from a database.

On the command line the same check is `--check-references`, where it exits 1
when orphans exist so it can gate a build.
"#;

pub const UNION: &str = r#"# Union Tables

Union Tables stacks two or more open tabs on top of each other into one
new table, like appending several exports of the same shape. Open it via
**Data > Union tables...**.

## How it works

Tick the tabs to combine. Octa builds a **reconciliation plan**: the
result has the union of all their columns. For each merged column you can
keep or drop it and choose its target type. Columns that appear in only
some tables are filled with empty cells for the rest. Mixed numeric types
widen to a common number type; otherwise the column falls back to text.

By default column names must match exactly, because to some downstream tools
a renamed-only-in-case column really is a different column. Tick **Ignore
upper and lower case in column names** to merge `Amount` and `amount` into
one column; the first spelling encountered names the result, so the output is
named the way one of the real sources spells it.

Apply opens the combined result in a new tab, leaving the sources
untouched. Also available as `octa --union` (with `--union-ignore-case`) and
the `union_tables` assistant/MCP tool (with `ignore_case`).

## Saving the result back in its own format

When every source shares one format, the result tab remembers it, and Save As
opens pre-filled with a matching name - so forty JSON files in, one JSON file
out, in a single click. A mixed selection has no single answer, so the picker
opens with no suggestion. Nothing is written until you save; Apply only opens
a tab. Note that nested JSON comes back flattened (one column per leaf, e.g.
`address.city`), because the union reconciles columns rather than trees.

## Union files straight from the sidebar

You do not have to open a tab per file first. In the directory sidebar,
**Ctrl-click** each file you want (**Shift-click** takes a whole run), or
**drag** across the rows to rubber-band them. Dragging to the top or
bottom edge scrolls the list, so a selection can run past the rows on
screen; hold **Ctrl** while dragging to add to an existing selection, and
a click that does not move still just opens the file.

Selected rows stay highlighted and an "N selected" bar appears at the top
of the sidebar; click **Union...** there, or right-click a selected file
and choose **Union selected files...**.

Octa reads the files and shows the same reconciliation plan, with one
checkbox per file instead of per tab. This is the quick way to stack a
folder of partitioned exports: forty part-*.parquet files become one table
without forty tabs. The files need not share a format, since the columns
are reconciled either way.

A plain click still opens a file and clears the selection. Unreadable files
are skipped and counted in the status bar.

Reading many files runs in the background, so the window stays responsive: the
status bar shows a spinner and a running count (`Reading files for union:
12/40`) until the dialog opens.

## Union files in the cloud

The same works in the cloud sidebar. **Ctrl-click** the objects you want,
then click **Union...** in the selection bar at the top of the cloud
section, or right-click a selected object and choose **Union** from the
context menu. Octa downloads them in the background and opens the same
reconciliation dialog, so a folder of partitioned parquet parts in S3,
Azure Blob or GCS becomes one table without a tab per object.

Both stages report progress in the status bar - first the download count, then
the reading count - so a slow bucket never looks like a freeze.

Whole folders go in one action, with no object-by-object ticking: right-click
a folder in the cloud tree and choose **Union tables in this folder...**, or
**Union tables in this folder and subfolders...** for a recursive sweep. Octa
lists the prefix, keeps the objects it can read, and unions those.

A folder union reads every file fully into memory, so it stops after 500 files
by default and the status bar reports how many were skipped. Change that
number, or tick **Unlimited**, under **Folder union file cap** in
**Settings > Performance**.
"#;

pub const JOIN: &str = r#"# Join Tables

Join Tables matches rows between two open tabs, like a spreadsheet VLOOKUP
or a SQL JOIN. You need a second table open in another tab first. Open it
via **Data > Join tables...** (Ctrl+Shift+Q).

## How it works

Pick the **left** table and the **right** table, then add one or more
**conditions**. Each condition pairs any column of the left table with any
column of the right table through an operator:

- `=` equal, `<` less than, `<=` less or equal, `>` greater than,
  `>=` greater or equal.

The columns do **not** need the same name, and their **types do not need to
match** - Octa converts both sides to a common type before comparing
(numbers when both are numeric, otherwise text). So you can join a numeric
`id` against a text `ref`, or match rows where one table's date is `>=`
another's. Add several conditions to require all of them (an AND join).

Then pick the join type:

- **Inner** - keep only rows that match.
- **Left** - keep every row of the left table, filling unmatched right
  columns with empty cells.
- **Right** - keep every row of the right table.
- **Full** - keep every row of both.

The matched result opens in a new tab. Joins run through DuckDB, so they are
fast even on large tables.

The command-line `octa --join` and the `join_tables` assistant/MCP tool
join on shared **column names** with equality (`--join-on`); the in-app
dialog is the place for different column names or non-equal operators.
"#;

pub const BATCH_CONVERT: &str = r#"# Batch Convert

Convert many files into one format in a single run: a folder of CSVs
into Parquet, a pile of JSON exports into Excel.

## Two ways in

- **From the sidebar**: Ctrl-click or Shift-click files in the folder
  tree, then press **Convert...** in the selection bar (or right-click a
  selected file). Works from one file up.
- **File -> Batch convert...**: pick a folder, and every file directly
  inside it becomes an input.

## The dialog

It shows how many inputs there are, a **Convert to** dropdown listing
only formats Octa can actually write, an output folder, and a **Replace
files that already exist** checkbox, off by default.

Conversion runs in the background with a live "Converting 3 of 12"
counter and a **Cancel** button. When it finishes, a **Batch convert
report** tab opens with one row per file: input, output, status
(done / failed / skipped / pending), rows written, and the error if any.
Cancelling leaves untouched items as `pending`, so the report always
says exactly what happened.

## Naming

Outputs are `<folder>/<input name>.<new extension>`, so `sales.csv`
becomes `sales.parquet`.

If two inputs would produce the same name, the later ones get `_2`,
`_3` and so on. Converting `/jan/data.csv` and `/feb/data.csv` into one
folder gives `data.parquet` and `data_2.parquet`, never one silently
overwriting the other. An output that already exists is skipped unless
you tick Replace.

## What it will not do

- **Local files only.** Cloud URLs are not accepted.
- **The first table only** for multi-table inputs: an Excel workbook
  with five sheets converts sheet one.
- **One file at a time.** Predictable and cancellable.

Compressed inputs (`.csv.gz`, `.parquet.zst`) are decompressed
automatically. One failed file never stops the run.

## Elsewhere

The same operation is on the command line as `--batch-convert --to EXT
--out-dir DIR`, which exits 1 if any file failed, and over MCP as the
`batch_convert` write tool.
"#;

pub const DATE_TIME_CALC: &str = r#"# Date/Time Calculation

Derive a new column from date, time or duration values, opened from
**Edit -> Date/Time calculation...** The new column is materialised in
place and is undoable.

Pick one of six operations; the fields below change to match.

- **Difference between two dates**: the gap between two date columns, as
  a number in the unit you choose.
- **Add / subtract time**: shift a date column forward or back by a whole
  number of units. A fraction is refused with an inline error.
- **Convert duration units**: the same duration expressed differently,
  for example milliseconds into seconds.
- **Extract a component**: pull out one field, such as the year, month or
  weekday.
- **Unix timestamp / date**: convert between an epoch number and a
  readable date/time, in either direction. The epoch is read as UTC.
  Nanosecond values keep full precision.
- **Convert timezone**: read each datetime as wall-clock time in one zone
  and write it as wall-clock time in another.

## Convert timezone

Choose a **From zone** and a **To zone** from the full IANA list. The
**Filter zones** box narrows both lists at once, so typing `Berlin` or
`America/` avoids scrolling 597 entries.

You have to state the source zone because Octa cannot detect it: it
stores datetimes without a timezone, so `2024-01-15 12:00:00` carries no
evidence of where it belongs.

Times that never happened, or happened twice, are **left empty and
counted**. Every zone with daylight saving has two such moments a year:
the clocks jump forward and an hour is skipped, then jump back and an
hour repeats. In `Europe/Berlin`, `2024-03-31 02:30` does not exist and
`2024-10-27 02:30` happens twice. Neither has a single right answer, so
Octa refuses to guess and reports how many cells it left empty.

## When a cell cannot be computed

Values that are not valid dates (for the date operations) or not valid
numbers (for duration conversion) are skipped and the new column is left
empty there, with a banner giving the count. Plain-text columns are read
through the same date inference the table uses, so ISO and common
European and US layouts work even when the column is still typed as
text.
"#;

pub const TIME_SERIES: &str = r#"# Time Series

Two reshapes over a time column, opened from **Analyse -> Time series...**
Both put the result in a **new tab**; the source table is untouched.

## Time buckets

Group rows into one bucket per **minute / hour / day / week / month /
quarter / year** and aggregate the value columns. Daily orders into
monthly totals.

- **Time column**: the timestamp to bucket. Date-typed columns are
  listed first in the dropdown.
- **Bucket size** and **Aggregate** (Sum / Mean / Minimum / Maximum /
  Count / First / Last).
- **Value columns**: what gets aggregated.
- **Separate series by**: optional, one series per combination.

The bucket lands in a column named `bucket` (or `bucket_2` if the table
already has one).

## Rolling window

Add a column holding the aggregate of the current row and the N-1 rows
before it: a 7-day moving average.

- **Order by**: the column that orders the frame. Required, because a
  rolling aggregate over unordered rows is meaningless.
- **Value column**, **Window (rows)** (the frame size, including the
  current row) and **Aggregate**.
- **Restart for each**: optional, the window never spans two groups.

The result is every source column plus `<value>_rolling_<window>`.

## Before you commit to it

The dialog shows a plain sentence describing what will happen, and a
preview: the operation run against the first 1000 rows, showing the
first 10 results. It never runs against the whole table just to preview.
**Create tab** stays disabled until the inputs work, and the note beside
it names what is missing.

## Elsewhere

The same two operations are on the command line as `--resample` and
`--rolling`, and over MCP as `resample_timeseries` and `rolling_window`.
All three surfaces build the same SQL, so the results agree.

One detail worth knowing: the time column is cast leniently, so a single
unparseable timestamp lands in an empty bucket rather than failing the
whole operation.
"#;

pub const CLEANUP: &str = r#"# Clean-up Suggestions

A panel that scans the open table for common data problems and offers a
fix for each one. It answers "what is wrong with this file?" without
you having to run six separate checks by hand.

Open it from **Analyse -> Clean-up suggestions**. There is no setting
to switch on: nothing runs until you open the panel.

## Scanning

**Opening the panel starts the scan**, and that is the only thing that
does. The scan runs on a background thread against a snapshot of the
table, so the window stays responsive, and **Cancel** stops it. It
examines the **first 100,000 rows**; when the table is longer, the
panel says so.

Closing and reopening the panel scans again, which is how you refresh
the list after editing the table by hand. Applying a fix rescans on its
own, since a fix can shift the column numbering the other suggestions
refer to.

The scan reuses the detectors the individual features already use, so
what it reports and what the matching dialog reports agree.

## What it looks for

| Problem | Severity | Fix |
|---------|----------|-----|
| Leading / trailing spaces in a column | High | Applied directly |
| Garbled characters from a wrong character set | High | Applied directly |
| Whole-row duplicates | High | Applied directly |
| A column that looks like personal data | High | Opens Anonymise |
| A text column whose values are all numbers | Medium | Applied directly |
| Numbers wearing a unit: 1.2k, EUR 4,00, 12 kg, 45% | Medium | Opens Split numbers |
| A column with 5% or more empty values | Medium | Opens Fill missing values |
| A completely empty column | Medium | Applied directly |
| Numeric outliers (IQR, k = 1.5) | Low | Opens Detect outliers |
| A column whose every row holds the same value | Low | Applied directly |
| Column titles that are not tidy identifiers | Low | Applied directly |

Results are ranked: highest severity first, then by how many rows or
cells the fix would touch.

### Columns that hold one value

A column where every row says EU separates nothing: a filter over it
answers the same thing every time, and a group-by returns one group.
Octa suggests dropping it.

**Nulls do not count as the value.** A column of 900 active and 100
empties is a column with missing values, not a constant one, and dropping
it would throw away the fact that some rows had nothing. A single-row
table is not reported either, since every column of one row is constant
by accident.

### Numbers wearing a unit

1.2k, EUR 4,00, 12 kg and 45% are text to every reader, so they sort
alphabetically, refuse to sum, and quietly poison any average taken over
them. Octa reports a column when at least 80% of its values split this
way, and at least three of them do.

Clicking the suggestion opens **Split numbers from units**, which changes
nothing until you press Apply and defaults to the answer that changes
nothing: leave as text, add a number column, or add a number column and a
unit column. **The original column is never touched**, so the value as it
was written stays in the file, and both new columns arrive in one undo
step.

Three details worth knowing:

- **A magnitude suffix is folded into the number.** 1.2k becomes 1200 and
  the unit is empty, because 1.2k is a number rather than a number of
  anything.
- **A percentage keeps its number.** 45% becomes 45, not 0.45. Turning one
  into the other is a change of meaning.
- **The decimal convention is decided over the whole column.** $1,200 on
  its own is genuinely undecidable: twelve hundred dollars in Ohio, one
  euro twenty in Bavaria. A column of them usually settles it, and getting
  it wrong would be wrong by a factor of a thousand. A column that mixes
  units says so, since that is the column nobody should sum.

## Seeing the evidence

Most rows carry a **Show** button that lists up to three of the real
offending values, so you can check what the suggestion means before
changing anything. Whitespace examples are quoted (`"Tokyo "`) because
a trailing space is otherwise invisible, and untidy headers read as
`Order ID -> order_id` so you see exactly what the rename would do.

Rows with nothing to show get no button: an empty cell and an empty
column have no value worth printing.

## Applying a fix

Under every suggestion is a line saying **what Apply would do to this
table**, naming the real column and count, for example:

    Apply: remove the spaces around 12 values in 'city'. Undo with Ctrl+Z.
    Apply: opens Fill missing values with 'notes' chosen, so you pick
    how to fill them.

So you can tell before clicking whether the fix happens straight away
or opens a dialog for you to decide in.

Each row has **Apply** and **Ignore**. Ignore just hides that row for
the session; nothing is remembered between runs.

Fixes split into two kinds:

- **Unambiguous fixes apply straight away** and are **undoable with a
  single Ctrl+Z**, because they run through the same code the manual
  menu entry runs. Trimming a column, casting it, dropping an empty
  column, snake_casing the titles, dropping duplicate rows.
- **Fixes needing a decision open the matching dialog**, pre-filled with
  the column in question. Filling missing values needs a strategy,
  outliers need a method and a threshold, anonymising needs an
  algorithm. The panel does not guess these for you.

Apply is disabled in read-only mode (**F8**).
"#;
