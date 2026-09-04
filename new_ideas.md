# New ideas for Octa

Fifty things Octa could add or do better, written at the start of version 0.19.
Nothing here is designed yet: each entry is a problem plus the smallest shape of a
fix, so one can be pulled into brainstorming without re-deriving the context.

Items marked **(parked)** already existed in
`docs/superpowers/specs/2026-07-13-feature-proposals.md` and were never built. They
are repeated here so the list is complete, not because they are new.

Nothing already rejected is repeated. See the last section.

## A. Grid and interaction

### 1. Column quality bar in the header **(parked)**

Under each column title, a 3px stacked bar: valid / empty / type mismatch, with a
mini histogram on hover. Every number is already computed by the Summary engine; the
point is seeing it without leaving the grid. Cheapest visible win on the list.

### 2. Chart to table brushing **(parked)**

Click a bar or drag a box in a Chart tab and the source tab's column filters narrow
to that slice. Reuses the filter machinery wholesale, and turns the chart from a
screenshot into a way to explore.

### 3. Totals row pinned at the bottom

One extra row below the grid, one function per column, chosen from a dropdown in the
column header menu (sum, mean, count, distinct, min, max, null count). The status bar
already computes this for a selection; this makes it permanent and per-column.

### 4. Row height and wrap toggle

A three-state control: one line, wrapped to a fixed number of lines, or auto. Free
text columns are unreadable in a one-line grid today, and the workaround is opening
the cell.

### 5. Cell inspector popout

Right-click a cell holding JSON, XML or a long text blob, open it in the JSON tree or
raw view in a scratch tab. Nested payloads in a column are common and currently show
as a truncated one-liner.

### 6. Split view of one table

Freeze a second scroll region so row 12 and row 900 000 sit on screen together. Frozen
columns already exist; this is the horizontal equivalent, and it is what people
actually use to compare two records.

### 7. Row diff

Select two or more rows, get the Compare view over just those rows, differences
highlighted. Compare exists for files, not for rows inside one.

### 8. Column mask jump

In a table with 400 columns, typing part of a column name in the header bar should
scroll it into view and flash it. Hidden columns and reorder exist; finding one does
not.

### 9. Excel selection muscle memory

Ctrl+Space selects the column, Shift+Space the row, Ctrl+Shift+Arrow extends to the
edge of the data block. People arrive from Excel with these in their fingers and lose
a few seconds every time one does nothing.

### 10. Export the current view to PDF

Chart tabs already export PDF. The table view, the Summary tab and the HTML report do
not, and "send me that as a PDF" is how these things travel inside companies. One
paginated renderer covers all three.

## B. Analysis and data quality

### 11. Benford's law check

A first-digit distribution test in the quality report, with a plain-language verdict.
Cheap to compute, and the standard smell test for hand-entered or massaged figures.
Must state clearly when the test does not apply (bounded ranges, small samples).

### 12. Distribution comparison between two columns or two files

Kolmogorov-Smirnov or chi-square with a sentence in front of it: "these two look like
the same population" or "the second file skews 12% higher". Data drift compares
across files at the table level; this is the column-level, statistically honest
version.

### 13. Referential integrity check

Given a declared or discovered foreign key, list the orphan rows: values in the child
that have no parent. The relationship map already knows the keys, so this is a scan
plus a result tab, and it is the first thing anyone asks after seeing the map.

### 14. Discovered cross-column rules

Scan for relationships that hold in almost every row: `end_date >= start_date`,
`net + vat = gross`, `country = DE` implies a five digit postcode. Present them as
candidate validation rules with their violation counts. Feeds straight into the
existing rules file.

### 15. Missingness patterns

Not "column X is 12% null" but "these four columns are null in the same 8 000 rows".
A small co-occurrence table answers "is this one broken import or four broken
columns", which per-column null counts never can.

### 16. Unit and currency normalisation

`1.2k`, `3 Mio`, `EUR 4,00`, `12 kg`, `5%` all read as text today. Detect the pattern
per column, offer a numeric column plus a unit column. European decimal parsing
already exists, so this is the same idea one layer up.

### 17. Text column profiler

For a string column: length distribution, character-class mix, and the most common
patterns as masks (`AA-9999`, `999.999.999`). Catches "97% match this shape, 300 rows
do not" better than any null count.

### 18. Duplicate column detection

Two columns with identical or near-identical content, one of them a leftover from a
join. Ships as a new clean-up suggestion kind with a drop action, next to the existing
empty-column one.

### 19. Constant column detection

A column with exactly one distinct value across every row carries no information and
is usually an accident of an export filter. Another clean-up suggestion kind, same
shape as the above.

### 20. Calendar coverage for time series

Given a timestamp column: missing days, irregular intervals, duplicated hours at DST
changeovers, gaps at weekends. The time-series tools bucket and roll, but nothing
tells you the series has a hole in March.

## C. Formats and readers

### 21. PDF table extraction **(parked)**

Read-only, one table per detected table, behind a visible "check this, extraction is
imperfect" banner. Financial reports, government statistics, invoices. Still the
format gap that bites hardest.

### 22. HTML table reader

`.html` files and `Open URL` pointing at a page: pull out `<table>` elements as
tables, one per table, same multi-table shape as Excel sheets. Wikipedia and internal
wiki pages are a normal data source and Octa cannot touch them.

### 23. Microsoft Access reader

`.mdb` and `.accdb`, read-only, one tab per table. Still everywhere in mid-sized
companies, still the thing nobody can open on Linux.

### 24. SQL dump reader

A `.sql` file with `CREATE TABLE` plus `INSERT` statements opens as tables. This is
how a colleague sends you a database when they cannot give you the server.

### 25. Log file reader

nginx and Apache access logs, syslog, and a generic "these lines all match this shape"
detector, turned into columns. Fixed-width and CSV sniffing already exist; this is a
third sniffing strategy, and it makes Octa useful to people who never touch parquet.

### 26. Delta and Iceberg time travel

When opening a table directory, offer the snapshot list and open an older version.
The readers already parse the metadata that lists the snapshots; only the picker is
missing. Pairs with the existing compare view for "what changed last Tuesday".

### 27. Excel formula round-trip

Read the formula behind a cell, show it in the cell inspector, and preserve it on save
instead of flattening it to a value. Styling preservation already went in for 0.17, so
the writer half of this problem is understood.

### 28. Streaming writes for large exports

Batch convert and harmonise materialise the whole output table before writing. Write
row groups as they are read instead, so a 40 GB conversion is bounded by disk, not by
RAM.

## D. Databases and SQL

### 29. Oracle engine

Nine engines, and the one most common in large enterprises is not among them. Same
one-file-per-engine shape as the other nine; the work is in the type mapping and the
client library choice.

### 30. SSH tunnel for database connections

A per-connection "reach it through this bastion host" section. Almost every managed
Postgres in a company sits behind a jump host, and today the answer is "open a tunnel
in a terminal first".

### 31. Query plan viewer

An Explain button in the SQL panel and on database tabs, rendering the plan as an
indented tree with row estimates. The engines all speak `EXPLAIN`; the value is not
having to leave Octa to find out why a query crawls.

### 32. Per-connection query history

Every query run against a connection, with its timing and row count, re-runnable with
one click. Snippets exist for queries you decided to keep; history is for the one you
ran twenty minutes ago and did not.

### 33. Row counts and sizes in the database tree

Lazily fetched, shown greyed until they arrive, cached per session. "Which of these
600 tables is the big one" is unanswerable from the tree today.

### 34. Trino and Athena engines

The lakehouse SQL front ends. Octa already reads Delta and Iceberg files directly, so
the engines close the loop for people whose lake is only reachable through a query
service.

### 35. Per-connection read-only flag

Global write protection is all or nothing. Mark the production connection read-only
in its own settings and let the staging one stay writable, so the safe setting is not
one people turn off for convenience.

### 36. Query timing readout

Rows scanned, rows returned, time spent, shown after every SQL panel run, with a
one-line hint when a query is slow for an obvious reason (no filter on a partitioned
table, a cross join).

### 37. Result set diff

Run a query, change something, run it again, diff the two result sets. Diff exists for
files and for database tables; the same view over two query results turns the SQL
panel into a verification tool.

## E. Command line, MCP and the assistant

### 38. Recipe: record, replay, export **(parked)**

Every transform, filter, join and clean the user clicks lands in a per-tab step list
that can be replayed on another file and exported as SQL, Python or an `octa`
invocation. The most valuable thing on this list, and the largest.

### 39. Recipe replay in the CLI and MCP

Once 38 exists, `octa --recipe steps.toml input.parquet` and the matching MCP tool
make the GUI's work runnable in a pipeline. Do not build 38 without planning this one.

### 40. Shell completions

bash, zsh, fish and PowerShell, generated from the existing clap definition and
installed by `install.sh`. The CLI has over thirty actions and nobody remembers them.

### 41. Quality report from the CLI

`--quality file.parquet` with an exit code, next to the existing `--check`. The engine
exists and only the GUI can reach it, which is exactly backwards for something you
want in CI.

### 42. Progress on long CLI runs

Batch convert over 500 files prints nothing until it finishes. A stderr progress line
with a count and an ETA, suppressed when stderr is not a terminal, so pipelines stay
clean.

### 43. Explain this file

One button, one paragraph from the assistant: what this file appears to contain, what
the columns mean, what looks odd. The context assembly already exists for chat; this
is a fixed prompt in front of it, and it is the first thing anyone wants from an
unfamiliar file.

### 44. Assistant-authored validation rules

Describe a rule in words, get a proposed entry for the rules file with its current
violation count, review it, save it. Keeps the user out of the file format while the
file stays the source of truth.

### 45. Explain this query

Paste or select SQL in the panel, get a plain-language description of what it does.
Useful on inherited queries, and it composes with the plan viewer in 31.

### 46. Token and cost meter

Per chat profile: tokens used this session, an estimated cost, and an optional budget
that warns before it is crossed. People run local and paid models side by side and
currently cannot see which one they are spending on.

## F. Performance, reliability and polish

### 47. Out-of-core scrolling **(parked)**

A DuckDB-backed virtual table so rows are never fully materialised and the row cap
stops being a cap. Large-file mode covers most of the pain since 0.18, which is why
this sits here rather than higher.

### 48. Session restore

Opt-in: reopen the tabs that were open when Octa last closed, including the crash
case. Pinned tabs and recent files already persist, so the missing piece is the tab
list plus its per-tab view mode.

### 49. Memory panel

Per-tab row count, column count and estimated bytes, with a "close the largest" action.
Ten tabs of parquet is a lot of RAM and nothing in the UI says which tab is the
expensive one.

### 50. Portable mode

Settings, connections and cache next to the binary instead of in the user profile,
enabled by a marker file. Locked-down corporate machines and USB sticks, and it costs
one branch in the config path resolution.

## Deliberately not repeated

Previously rejected, do not re-propose: group-by, command palette, watch and
auto-reload, smart paste, filter and sort presets or any saved-view bundle, scripting
in transforms or anything else that makes the user learn a syntax, an Excel-style
import wizard, data-dictionary sidecars, folder-wide find and replace, Google Sheets,
shortened tab titles, and OS file drag-and-drop.
