This release adds large-file mode, relationship maps, data drift, validation
rule files, several database write paths, workbook export, URL input, and a set
of fixes.

## Features

### Large-file mode

Files that will not fit in memory open in **large-file mode**: read-only, rows
left on disk, only the page on screen fetched. Scrolling, sorting, filtering,
search, Summary, Chart, the SQL panel, export and convert work; everything that
changes data is off, and the notice shown before opening lists both sets.

The mode is chosen when the file is opened. Two conditions trigger it: the file
is at least the **large-file threshold**, which defaults to 10 GB and is set
under **Settings -> Performance**, or it states a row count at or beyond the
initial-load cap. The notice itself can be turned off there as well, or from its
own tick box.

Parquet, CSV, TSV and JSON are read where they lie. Other formats are converted
to a temporary Parquet file first; Octa says so before starting and the
conversion can be cancelled.

Headless, `octa --sql huge.parquet -q '...' --stream` scans the file in place,
so an aggregate covers every row. Four MCP tools answer from the file without
loading it (`count_rows`, `schema`, `read_table`, `run_sql`) and mark those
answers `streamed: true`.

### Relationship map

**Analyse -> Relationship map...** draws one box per table and a line between
each pair of columns that relate, with a sentence on every line describing how
well they match. The source can be the open tabs, a folder, or a live database,
where it reads the foreign keys the server declares from the catalog without
reading rows; **Measure** then scores those declared keys against the data. The
score threshold is a slider with a box for typing an exact value, and **Use in
Join** carries a line into the join dialog.

A map exports as PDF, PNG, SVG or interactive HTML, keeping node positions, line
bends, expanded columns and the theme colours.

The same catalog read is available to agents as the read-only
`db_relationships` tool, and `octa --relationships DIR` ranks the tables in a
folder from the command line.

### Join key finder and join diagnostics

The join key finder reports **orphan counts in both directions** on every
candidate rather than one side. This separates candidates that score
identically, which happens whenever both tables number their rows from 1: with
1,000 orders and 4 customers, `orders.id -> customers.id` and
`orders.customer_id -> customers.id` both score 1.00, and only the orders side
shows that 996 order numbers point at no customer. The result no longer depends
on which table was opened first. `suggest_join_keys` returns both counts.

Both features are documented in full: the scoring arithmetic, why the ranking
takes the larger of the two distinctness values, and two worked examples small
enough to check by hand. Each fix that join diagnostics suggests is the match
count recomputed with one normalisation applied, listed only when it strictly
beats the current count. Neither feature consults a language model; the privacy
page now states which three features do.

### Data drift

**Analyse -> Data drift...** compares two versions of the same data and reports
what moved: null rates, distinct counts, min, max, mean, and category values
that appeared or vanished. It answers whether the shape of the data still
matches, rather than which rows changed, which Compare and `--diff` already
cover. Change is reported relative to the baseline, so one threshold applies
across columns of different size. The category limit defaults to 50 and is a
field in the dialog.

`octa --drift-report A B --fail-on ...` runs the same comparison and exits 1
when a threshold is breached; `data_drift` is the MCP equivalent.

### Validation rule files

Data validation rules can be written to a TOML file with **Save rules...** and
read back with **Load rules...**. Rules are stored by column name rather than
position. A file naming a column the table does not have is reported, with the
columns named, rather than partly applied.

The same file runs headless: `octa --check orders.parquet --rules quality.toml`
exits 1 on any failing rule and on any rule that could not run. Agents call
`check_rules`.

### Database writing

- **File -> Save to database...** writes the active tab, whatever its source,
  into a database as a new table: create, replace or append, targeting a saved
  connection or a DuckDB or SQLite file. Column names are written exactly as
  they appear in Octa, capitals included, and pending cell edits are included.
  A large-file tab is refused, since it holds one page of a larger file.
- **Tables without a primary key can be edited.** A UNIQUE constraint whose
  columns are all NOT NULL is accepted as a row key. Without one, on PostgreSQL,
  MySQL, SQL Server, Redshift and Exasol, a save matches rows on all their
  values; each statement is checked to have affected exactly one row inside the
  transaction, and two matches or none aborts the save. A banner on the tab says
  which of the three cases applies.
- **File -> Save SQL...** writes the statements a save would run to a `.sql`
  file and sends nothing to the server, for workflows where changes are reviewed
  as a script first. `octa --sync-sql file --db conn --sync-table t --sync-on id`
  is the headless equivalent, printing the transaction to stdout and the counts
  to stderr; `sync_sql` is the read-only MCP version. All three use the same
  renderer as the write-back itself.
- **Confirm database write-back** (**Settings -> Databases**, on by default) can
  be switched off. It controls whether the confirmation appears, not how the
  write runs: one transaction, rolled back on failure, either way.

### Workbook export

**File -> Export workbook...** writes any number of open tabs into one `.xlsx`,
one worksheet per tab. Sheet names start from the tab labels and stay editable;
before writing they are trimmed to 31 characters, stripped of the punctuation
Excel forbids, and numbered where they collide. `octa --to-workbook out.xlsx
a.csv b.parquet` and the `write_workbook` tool do the same headless.

### Files from a web address

**File -> Open URL...** downloads an `http`/`https` address in the background
and opens it in a new tab. Every other place that takes a file also takes a URL,
including the CLI. When the link redirects elsewhere, Octa shows both addresses
and asks before opening; the question is controlled by **Settings -> Files ->
Ask about redirects**, on by default, and turning it off asks for confirmation
because the dialog is the only place a changed destination is shown. A public
address redirecting to a local or private one is refused. URLs supplied by an
agent are restricted to globally routable hosts and are not followed through
redirects.

### Partition layouts

Writing a partitioned table offers four layouts: flat files, a folder per value,
Hive `column=value` folders, and Hive folders with numbered part files. A live
preview shows the paths that would be written, built from the column's own
values. All four reopen as one table with **File -> Open table folder...**. The
option is available in the GUI, as `--partition-layout`, and over MCP; flat
remains the default.

### Piped input and output

`-` means standard input where a file is expected, and standard output as the
`--convert` target. The format is determined from the bytes rather than a file
name:

```bash
curl -s https://example.org/sales.csv | octa --schema -
cat sales.csv | octa --convert - - --to json | jq '.[0]'
```

`--to` is required when writing to `-`. Counts and notes go to stderr, so the
next command in the pipeline receives only the data.

### Smaller additions

- **Duplicate column names can be repaired on request.** Renaming by name cannot
  address a repeated name, so **Fix duplicate names** in the rename dialog (also
  **Columns -> Fix duplicate names...**) works by position: the first column
  keeps the name, later ones become `id_2`, `id_3`, and a suffix another column
  already holds is skipped. It is a tick box with a preview of what it would do,
  off unless ticked, and **Ignore upper/lower case** extends it to treat `Name`
  and `name` as the same name. The batch is one undo step.
- **Format JSON** in the Raw view breaks a minified file into indented lines.
  Only the whitespace between tokens changes, so numbers keep their exact digits
  and keys stay in file order. It does not mark the tab as changed, and
  un-ticking restores the file text.
- **Ambiguous date columns can be answered once.** The dialog reports how many
  columns are still queued and offers **Use this answer for all remaining
  columns**, which settles every queued column that offered the layout chosen;
  columns with different candidates are still asked separately.
- **MS SQL Server** is the tenth schema-export target: T-SQL types,
  bracket-quoted identifiers.
- **Documentation** for all of the above, in the in-app help and on the site,
  plus a page on how the assistant reads your data, and tests that fail when a
  shortcut, CLI action, export target or MCP tool is undocumented.

## Fixes

- **PostgreSQL `numeric` columns were read as NULL, and a write-back could then
  set them to NULL on the server.** The type has no decoder unless a decimal
  crate is compiled in, and the resulting error was being swallowed. Octa now
  decodes the binary wire format into exact decimal text, without passing
  through a float.
- **The Raw view re-tokenised the whole buffer every frame.** egui calls a text
  layouter before consulting its galley cache, so syntax highlighting ran on
  every frame: a 700 KB JSON file cost 4.3 seconds per frame. The result is
  memoised, and JSON, YAML, XML and TOML are highlighted again, having been
  excluded to work around this.
- **Save on a database tab opened the Save As file picker.** A database tab
  carries no file path, so save paths that tested for one routed to a file
  dialog instead of the write-back. They now share one predicate.
- **Closing a tab during a write-back could discard the edits being written**,
  or retag another tab, because the close shifted the index the in-flight job
  held.
- **Schema-export DDL and its INSERT could disagree on a column name.** The
  `CREATE TABLE` quoted identifiers conditionally while the INSERT always quoted
  them, so an engine that folds unquoted identifiers (PostgreSQL to lower,
  Snowflake to upper) created a column the INSERT could not address.
- **Formatting-only differences produced UPDATE statements.** Numbers are
  compared as numbers, so a file's `120.50` and a `numeric(12,2)` column's
  `120.5` are no longer reported as a change.
- **A load banner listing many columns pushed its own buttons off screen.** A
  file with dozens of ambiguous date columns produced one line wider than the
  window, in a strip that does not scroll sideways. The banner now names six
  columns and counts the rest, with the full list on hover.
- **Sheets of one workbook all carried the same tab label.** Three sheets of one
  file were three tabs called `book.xlsx`; they are now `book.xlsx - Costs`. The
  same applies to a table chosen from a SQLite or DuckDB file. A renamed tab
  keeps its own label.
- **A filtered large-file tab showed its column headers over an empty grid.**
- **Tables with a usable unique constraint opened read-only.** See the
  write-back changes above.
- **Declared foreign keys pointing at a table outside the scanned schemas** are
  counted and reported rather than dropped.
- **File menu order**: the entries that save the open table, then a separator,
  then the entries that export something derived from it. Export workbook
  previously sat between Save and Save as. **Data validation** moved from
  **Columns** to **Data**.
  