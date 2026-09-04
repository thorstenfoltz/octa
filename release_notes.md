Three more database engines, a way in through a jump host, live tables that
page instead of stalling, a paginated PDF export, two new analyses, two new
formats, and an assistant that costs about a fifth of what it used to per
message. Read the breaking changes before upgrading if you script Octa, read
SAS files, or run the MCP server read-only.

## What's new

### Three more database engines: Oracle, Trino and Athena

Octa now connects to **twelve** live engines.

**Oracle Database** (12.1 and later) is spoken over its own TNS protocol in
pure Rust, so there is **no Instant Client to install**: nothing beside Octa
itself. Browse schemas and views, read tables, follow declared foreign keys,
edit and write back. The **Database** field holds the *service name*
(`FREEPDB1`), not a database name. Two limits are the driver's rather than the
server's and are documented in full: `BINARY_FLOAT` / `BINARY_DOUBLE` are not
decoded, and a rejected statement drops the connection instead of reporting
Oracle's own `ORA-` text. Oracle is also the one engine where a running query
cannot be cancelled, because killing a session there needs `ALTER SYSTEM`.

**Trino** connects over the HTTP statement API, with password, browser SSO or a
personal access token, and browses every catalogue rather than only the default
one.

**Amazon Athena** connects over its JSON API with every request signed with
SigV4. It takes the Glue database, a workgroup, and an S3 result location
unless the workgroup already sets one.

Copy table works between any two of the twelve, in either direction.

### Reaching a database through a jump host

A connection can now carry its own SSH tunnel, so a database that only answers
from inside the network no longer means opening a tunnel in a terminal first.
Tick **Reach this database through a jump host** on the connection and give the
bastion, your account and how you sign in: **SSH agent** (nothing to fill in),
a **private key file** (passphrase kept in the keyring, separate from the
database password), or a **password**.

One tunnel is shared by every tab, query and write on that connection.
**Test connection** goes through it too, so a bad bastion reports as an SSH
error rather than a confusing database one. `--db-*` on the command line and
the database MCP tools read the same saved connection, so they tunnel with no
extra flags.

Host keys are checked against your own `~/.ssh/known_hosts`. An unknown host is
refused unless you tick **Accept a new host key**; a **changed** key is always
refused, whatever that box says.

The connection keeps naming the real database, so TLS certificates and Entra
token audiences are unaffected by tunnelling. ClickHouse over HTTPS and Exasol
are the two exceptions, and the docs say why.

### Live database tables load a page at a time

Opening a table from the sidebar used to read up to the initial-load row cap in
one request and sit there while it did. It now reads one page, shows it, and
fetches the next in the background as you scroll, exactly as a large Parquet
file does. The status bar shows the count so far with a `+`, and says
"Loaded all N rows" when the table runs out.

**Settings > Performance > Live database page size** sets the page, 100,000
rows by default. It is its own knob because the initial-load cap sizes a local
file read, while this is megabytes of JSON crossing a network, and Databricks
refuses any single result over 25 MiB.

While a table is opening, or fetching its next page, the status bar carries a
spinner and a **Cancel** button that uses the engine's own cancellation, so the
warehouse stops working, and billing, too. A cancelled read keeps the rows it
already has.

### Per-connection query timeout

**Settings > Databases > Query timeout**, 60 seconds by default. It appears
only for Trino, Athena, Snowflake, Databricks and BigQuery, the five engines
that submit a statement over HTTP and then poll for it, so where to stop asking
is Octa's decision rather than a driver's. Timing out names the number of
seconds and points at the setting. `--add-connection` takes it as
`query_timeout=`.

### Compare distributions

**Analyse > Compare distributions...** answers one question: do these two
columns look like the same population? Last month against this month, control
against treatment, June's vendor file against July's. A mean and a standard
deviation answer it badly, since two samples can share both and be shaped
nothing alike.

Also on the command line as `--compare-distributions`, and as the
`compare_distributions` MCP tool.

### Referential integrity

**Analyse > Referential integrity...** names the child rows pointing at a
parent that is not there, before a join silently drops them. Both pickers start
on the tab you opened it from, so a self-reference (`manager_id` against `id`
in one table) needs no extra clicks.

Also `--check-references` on the command line, which exits 1 when orphans are
found, and the `check_references` MCP tool.

### Compare rows

**Analyse > Compare rows...** puts the rows you picked side by side, one output
row per column, with a **differs** column to run your eye down. Selection wins
when it holds two or more rows, otherwise marks are used, so a set of marks you
built on purpose is not replaced by a stray click. Works for any number of
rows, not just two.

### The data quality report grew three findings

The **score** is now explained rather than asserted: 40% completeness, 40%
type consistency, 20% distinct ratio, up to 10 points off for outliers. It
appears in the tab title (`Quality 81/100 - sales.parquet`) so it stays on
screen, and hovering the tab or the `score` header explains what it means.

**Benford's law** (`benford_verdict`) asks whether a numeric column's leading
digits look like measured quantities. Four gates come first, and a column that
trips one says so instead of getting a verdict it had no reason to follow: not
numeric, fewer than 300 values, a range narrower than a factor of ten, or a
dense assigned sequence such as an id. The gate tests density rather than
uniqueness, so Fibonacci numbers stay in and a run of 1 to 1000 does not.

**Calendar coverage** (`calendar_verdict`) infers a time column's step and
looks for holes in it. Two false alarms are ruled out rather than reported: a
weekday-only series reads `weekdays only`, not 104 gaps a year, and a
daylight-saving change is recognised by its shape and gets its own verdict.
The holes themselves open in a **Calendar gaps** tab, one row per gap, capped
at 200.

Both verdict columns are written to be read rather than decoded, and
**hovering a verdict explains it**: what it is telling you, and what it is not.
A column the test does not apply to says `not tested: narrow range` rather than
passing judgement on a percentage by a law percentages have no reason to
follow. The column header keeps its own tooltip for the column as a whole.

**Missing together** opens as its own tab: which sets of columns are empty in
the same rows. A single column on its own is not a pattern, and neither is a
set covering fewer than 1% of the table or fewer than 5 rows.

Findings with nothing to report open no tab, so a clean file still gives you
just the score.

### Two more clean-up suggestions

**Numbers wearing a unit.** `1.2k`, `EUR 4,00`, `12 kg`, `45%` are text to
every reader: they sort alphabetically, refuse to sum, and poison any average
taken over them. Clicking the suggestion opens **Split numbers from units**,
which defaults to changing nothing and offers to add a number column, or a
number column and a unit column, beside the untouched original. Both arrive in
one undo step. A magnitude suffix folds into the number (`1.2k` is `1200`), a
percentage keeps its number (`45%` is `45`, not `0.45`), and the decimal
convention is decided over the whole column rather than per value, because
`$1,200` on its own is genuinely undecidable.

**Columns that hold one value.** A column where every row says `EU` separates
nothing. Nulls do not count as the value, so a column of 900 `active` and 100
empties gets the missing-values suggestion instead, and a single-row table is
never reported.

### Export to PDF

**File > Export to PDF...**, or a tab's right-click menu, prints what the
active tab is showing to a paginated PDF: the grid, Summary, the quality
report, a comparison, any result tab. Exactly what you can see and nothing you
cannot, which means the rows the filter leaves, in the sort order on screen,
the visible columns in their current order, and colour marks and conditional
formatting in the grid's own palette.

### Two more formats: HTML and SQL dumps

**HTML** is read-only and needs no setting. Every `<table>` on the page becomes
a table, the way every sheet of a workbook does, named after its `<caption>`
where there is one. The parser is the one browsers use, so real tag soup parses
like a page rather than failing like strict XML. `rowspan` and `colspan` expand
into repeated cells, a leading row of `<th>` becomes the header, and a table
whose cells hold nothing but another table is treated as a layout wrapper and
skipped. **File > Open URL** hands a fetched page straight to it, so a
Wikipedia article opens as its tables.

**SQL dumps** are read-only and **off by default**, since a `.sql` file is
usually text and opens as text. Pick the reader by name with **File > Open
as > SQL dump** or **View > Reopen as > SQL dump**; a `.sql` that turns out to
hold `CREATE TABLE` and `INSERT INTO` says so in the status bar and points at
the second of those. `mysqldump`, `pg_dump` and `sqlite3 .dump` output all
work: rather than parse SQL, Octa scrubs the dialect-only spellings off each
statement and replays the file into a scratch database, then reads it like a
`.sqlite` file, table picker and all. `COPY ... FROM stdin` blocks, MySQL's
`/*!40101 ... */` comments, `AUTO_INCREMENT`, `enum(...)`, `ENGINE=InnoDB` and
backslash escapes are handled. A statement that will not replay is skipped
rather than fatal, and only failures that carry schema or data are counted,
with a banner quoting the first.

### Excel formulas

A cell whose value came from a formula shows the formula on hover, and the
Record view lists it beside the field. Reading needs no setting.

**Settings > Files > Write options > Excel > Keep Excel formulas when saving**
writes them back, off by default because Excel recalculates on open, so the
number in the saved workbook can differ from the one you were looking at. Octa
writes the value it has as the formula's cached result, so a tool that does not
evaluate formulas still sees the right number. Two things retract a formula
whatever the switch says: **a cell you edited**, and **a table whose rows or
columns you moved, added or deleted**, because `=B2*C2` no longer points where
it did.

### Split view: up to six bands, each scrolling on its own

A split used to be two bands that shared one scroll axis, so the second band
often showed the cells you were already looking at. Now **every band scrolls on
its own, both up and down and left and right**, and each has its own
scrollbars: dragging one moves that band and no other. Band 1 can sit on the
first columns of row 12 while band 2 reads the last columns of row 900,000.

**View > Add pane** cuts one more band out of the split, up to **six**, and
**Remove pane** takes one away. Switching between stacked and side by side
keeps the count, so four bands stay four.

Hold **Alt** and the wheel moves every band at once, for walking several bands
down the table in step. Alt+Shift+wheel does it sideways.

### Ask SQL knows how your tables join

On a database tab set to run on the server, the SQL panel's plain-language
**Ask** box is now given the tables one foreign key away from yours: their
names, their columns, and the key pair each join uses. Asking "who spent the
most" gets a query with the right join instead of a guess.

Those come from the database's own catalogue, so it costs two catalogue queries
and reads no rows from any table. The model is told to join only on a listed
key pair, never to invent one, and that a join multiplies rows so it should not
sum your table through one. The list is capped at eight tables and forty
columns each. A database that declares no foreign keys, a catalogue your
account cannot read, and a local (DuckDB) query all fall back to the
single-table question exactly as before.

### The SQL panel remembers, and times, what you ran

**History** now lists the queries you actually ran, each with how long it took
and how many rows came back, and it **survives restarts**. It is scoped rather
than global: a database tab records against its connection and a file-backed
workspace against its file, so queries you ran on production do not turn up
while you are poking at a CSV. **Settings > Databases > Query history** keeps
the last 20 per connection by default (0 keeps them all); switching it off
stops recording **and deletes what was kept**, because a query can carry values
out of your data. **Clear history** does the same on demand.

The result counter now carries the elapsed time beside it
(`1234 result rows (84 ms)`), covering the round trip for a live connection.
A query that fails is timed too, so a slow statement ending in an error still
tells you where the minute went.

### The assistant costs about a fifth of what it did

Describing all 62 tools is roughly 33,000 tokens, sent on **every** request
whether or not a tool is used. Octa now sends the **core** group in full, about
6,500 tokens (reading, schemas, counting, search, profiling, SQL), and lists
the rest for the assistant by name and group. It loads a group when a job calls
for one, which you see happen as an `enable_tools` step. One extra round trip
on the questions that need it, around 26,000 tokens saved on every request that
does not. Loading a group does not spend a tool iteration, since nothing was
done with the data.

**Prompt caching** marks the unchanging parts of a request, the tool
definitions, the system prompt and the conversation up to the last message, as
cacheable for Anthropic. OpenAI and Google cache long prompts by themselves.

The panel header **counts the tokens** this session has used, input and output,
exactly as the provider reported them. Nothing is estimated, and Octa puts no
price on them: rates change per region and per contract, and a bill you can
check beats a guess.

**Settings > Chat / Assistant > Assistant tools** lists every tool, grouped,
with what its description costs and a running total. Switching one off removes
it completely: not sent, not named in the group listing, and refused if the
assistant calls it from memory anyway. To stop the assistant writing anything,
use **Allow writes** on the profile instead, which covers every write tool at
once.

### Explain this file

**Analyse > Explain this file**, or the button beside the chat panel's
**Send**, asks one fixed question about the active tab: what the data appears
to be, what the columns mean, and anything odd. It is a normal chat turn, so
you can follow up on it, export it, or copy it; the panel opens itself if it
was closed and leaves a half-written message where it was. Greyed out until you
have a model profile.

### Your data is data, not orders

A cell, a column name or a file name can read like an instruction, and it
reaches a language model looking much like your own message does. Octa's system
prompt now tells the assistant that everything a tool returns is content rather
than instruction, and that only what you type is a request. That is a seat
belt, not a wall; the limits that do not depend on the model behaving are the
ones documented beside it, and a read-only profile removes the question
entirely.

### The MCP server can advertise fewer tools

An MCP client reads the tool list once and then carries it in every request to
its model, so a server you only use for reading Parquet was still charging you
for `fuzzy_join` on every message.

`--mcp-tools` advertises only what you name, taking group names and tool names:
`--mcp-tools core`, `--mcp-tools core,databases`,
`--mcp-tools read_table,run_sql`. `--mcp-without` is the other direction, and
the two combine. The groups are `core`, `quality`, `compare`, `combine`,
`reshape`, `databases`, `cloud` and `write`, and the tool reference now lists
which group every tool is in and roughly what each costs.

A tool that is not advertised is also not callable. An unrecognised name
**stops the server** rather than starting one with a surface you did not
intend, and the startup banner says how many tools were hidden.

It is a flag rather than a setting because an MCP server is launched by its
client, from that client's own config, and two clients pointed at one Octa
install often want different surfaces.

### Save asks when the file changed underneath you

Octa remembers each open file's modification time and size and checks them
again before **Save** writes over it. If a nightly job, an export or a
colleague on a shared drive has rewritten the file since you opened it, Octa
stops and offers **Save anyway**, **Reload**, or **Cancel** rather than
replacing their version with a snapshot taken before it existed.

Only **Save** is guarded: **Save As** writes where you point it, and a file
that was *deleted* is not a conflict, saving recreates it. Auto-save never
raises the prompt, it skips such a tab and leaves the question for your next
manual save.

### Status messages have one timeout, and a way to stop it

**Settings > Appearance > Message timeout** sets how long the line under the
toolbar stays before fading, ten seconds by default, the same span for every
message. Two rules replace the old confirmation-versus-error split: **hovering
pauses the countdown**, so an error you are selecting in order to copy stays
put, and **every message carries an `x`**. There is no off switch, and anything
below three seconds is treated as three, because a message that never expired
would cover the status bar until Octa restarted.

### Octa fits in a small window

The window can be dragged down to 400 x 300, small enough to park beside
another one, and everything stays reachable. Docked panels (SQL, Assistant,
Multi-search, Clean-up) never take more than two thirds of the window, so the
table keeps its share; the toolbar, tab bar and status bar scroll sideways
under the wheel when their contents no longer fit; and dialogs are kept inside
the window instead of opening partly off-screen. The **Initial window size**
setting now ranges from 400 x 300 up to 7680 x 4320.

### Ctrl+C copies the text you marked

The table takes over Ctrl+C so it can copy marked cells, which was wrong the
moment the marked thing was *text*: a chat bubble, tool output, a message in a
dialog, a focused text box. Every clipboard-hijacking site now stands down when
text is selected, so copying an error message out of a dialog works. Clicking
outside the text hands the shortcut back with the same gesture that stops the
text looking selected.

### The command line reports progress, and completes itself

`--batch-convert` over 500 files used to print nothing until the run finished,
so a slow file and a hung one looked identical. The looping actions now rewrite
a one-line count, current item and ETA on stderr, and stay **silent when stderr
is not a terminal**, so a pipeline gets the same summary lines it always got.
stdout is untouched.

`octa --completions bash|zsh|fish|...` prints a completion script, generated
from the same clap definition the binary parses with, so a new flag is covered
the moment it is added. `install.sh` writes the files for bash, zsh and fish,
skipping any directory it cannot write; `eval "$(octa --completions zsh)"`
works anywhere.

### Write options are grouped by format

**Settings > Files > Write options** now holds one group per format, Parquet,
CSV / TSV and Excel, instead of one flat list of every control. The two Excel
switches had no heading at all before.

### Every menu entry can have a shortcut

Nineteen menu entries had no bindable action, so there was no way to put them
on a key: correlation, compare distributions, referential integrity,
transpose, compare rows, random sample, tidy up, date/time calculation, export
to PDF, open directory, compare with git version, explain this file, report AI
content, the cloud and databases panels, and the split-view entries. All of
them are now in **Settings > Shortcuts**, unbound until you choose a key.

### The database read-only warning behaves like every other message

Opening a table from a connection that cannot be written to explained why in a
banner that stayed until clicked and looked like nothing else in the app. It is
now an ordinary status message: same style, same dismiss button, same pause
while the pointer is over it, and it fades after the time set in
**Settings > Appearance**. The `[Read-only]` pill in the status bar still
stands for as long as the tab is open. The "no key at all, rows matched on all
their values" note works the same way.

### A sample file for every format

The repository now carries `samples/`, one small openable example of every
format Octa reads: `tables/` holds the same eight rows in every tabular format,
plus databases, archives, documents, geometry, a Delta table and a dataset
directory. Open the folder in the sidebar and click down the list. Binaries go
through Git LFS.

## Fixes

- **Large Snowflake and BigQuery results were silently truncated.** Every HTTP
  response was capped at 10 MB by the client's default, so a big result came
  back short with no error. The cap is now 256 MB, shared by every connector,
  and exceeding it is reported rather than swallowed.
- **Databricks refused large results.** Statements now ask for
  `EXTERNAL_LINKS`, whose response carries presigned chunk URLs that Octa
  downloads, instead of the inline array Databricks caps at 25 MiB.
- **An unverifiable update no longer installs.** A `SHA256SUMS` file that
  cannot be fetched now aborts the in-app update, as a mismatch already did.
  The update never proceeds unverified.
- **Writing ORC panicked on types its encoder cannot handle.** Those columns
  are written as text now; the types ORC encodes natively keep their type.
- **DBF could not write a date column** whose type name was spelled a
  different way. It can.
- **Reading a SAS `TIME` column** reported `Float64` while showing `HH:MM:SS`.
  See the breaking changes below.
- **Many SAS date columns arrived as plain numbers.** See the breaking changes
  below.
- **Typographic punctuation in the locale catalogues** rendered as tofu in the
  GUI, whose bundled font has no glyph for it. The catalogues were swept and a
  guard test now fails on an em dash, a single-character ellipsis or an arrow.

## Breaking changes

### SAS time columns are now text, not numbers

A SAS `TIME` column has always been shown as `HH:MM:SS`, but Octa reported its
type as `Float64`, because the old type detection only looked for date and
datetime formats and let everything else fall through to a number. Time columns
now report `Utf8`, which is what the cell has always actually contained.

If you sort, filter, export or run SQL against a SAS time column, it now
behaves as text. Nothing about the values on screen changes.

### More SAS date columns are recognised as dates

Octa used to decide whether a numeric SAS column held a date by matching the
start of its format name against a hand-written list (`DATETIME`, `YYMMDD`,
`MMDDYY`, `DDMMYY`, `E8601DA` and a few others). Any date format outside that
list was read as a plain number.

The reader now takes that decision from the file itself, using the column's
format together with its internal flags. Date and datetime columns that used to
arrive as `Float64` now arrive as `Date` and `DateTime` and are displayed as
dates. This is a fix, but it changes the type of columns you may have been
treating as numbers.

### `--mcp-read-only` now drops more tools

The read-only surface used to omit six tools. It omits fifteen: `write_table`,
`write_workbook`, `edit_table`, `convert`, `batch_convert`,
`transform_columns`, `anonymize`, `partition_table`, `create_report`,
`harmonise_schemas`, `write_db_table`, `copy_db_table`, `copy_object`,
`move_object` and `delete_object`. Cloud object writes and live-database writes
were reachable before and are not now. An agent wired to a read-only server
that called any of the nine new omissions will get an error instead of a
result.

### Live database tables page instead of loading the row cap

Opening a table from the sidebar used to read up to the initial-load row cap in
one request. It now reads **Live database page size** rows (100,000) and
fetches the rest as you scroll. A script or a habit that relied on the whole
cap being present the moment the tab opened will see fewer rows until it
scrolls. The CLI and the MCP server cannot scroll, so they still use the
initial-load cap.

### SQL query history is written to disk

History used to be per tab and lost on exit. It is now kept in
`sql_history.json` in the config directory, scoped per connection and per file,
with the last 20 entries each. It is on by default. Switch off
**Settings > Databases > Query history** if you do not want queries stored;
doing so also deletes what was already kept.

### Building from source needs Rust 1.95

The minimum supported Rust version rises from 1.92 to 1.95, because egui 0.36
requires it. This affects only people who build Octa themselves; the published
binaries, the AppImage, the Docker image and the Microsoft Store package are
unaffected.

## Under the hood

- **SAS files parse faster.** The reader moved to `sas7bdat` 0.8, which reads
  the file through a memory map and decodes with SIMD instructions rather than
  byte by byte. Nothing about how you open a SAS file changes.
- **The interface toolkit moved to egui 0.36.** Window decorations now follow
  the application theme, and a press that slides off a control counts as a drag
  rather than a click, which makes dragging column edges and window borders
  less fussy.
- **The agent server moved to version 3** of the Model Context Protocol
  library. Every tool keeps the same name, arguments and output.
- **The build now fails on a known security advisory** in any library Octa
  depends on, not only on an incompatible licence, and prints every advisory
  that is deliberately deferred along with the reason.
- **One duplicate library was removed** from the binary: two different versions
  of the same zip implementation were being compiled in, and now only one is.
- **Oracle, SSH and HTML added no bloat by the usual measure.** Oracle speaks
  TNS in pure Rust rather than wrapping the Instant Client, the SSH tunnel uses
  a pure-Rust client rather than libssh2 or an `ssh` binary on PATH, and the
  HTML parser and the PDF page assembler were already in the lockfile as
  transitive dependencies.
- **The long files were split.** The CLI, toolbar, settings dialog, SQL view,
  relationship map, in-app documentation and application state are now
  directories of one file per menu, section or job.
