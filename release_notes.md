# Release notes

This release is a large one. Octa learns to point out what is wrong with a
table before you go looking, to work on a whole folder rather than one file at
a time, to join tables whose values almost match, and to answer a question
typed in plain English. Everything new is reachable from the app, the command
line and the assistant unless a section says otherwise.

## Octa tells you what is wrong with the table

**Clean-up suggestions** is a new panel under **Analyse**. Opening it scans the
table and lists what looks wrong: stray spaces around values, a column of
numbers stored as text, empty columns, duplicate rows, untidy column titles,
personal data, outliers, and text that was decoded with the wrong character set.
Each entry says in one sentence what the problem is, what Octa would do about
it, and shows up to three of the real offending values so you can judge for
yourself. **Apply** fixes it, and a single **Ctrl+Z** takes the fix back.

Nothing runs while the panel is closed, and nothing is detected twice: every
suggestion is a translation of a check Octa already had, so the panel and the
menu entry can never disagree about your data.

**Problem navigation.** **F10** and **Shift+F10** step through the cells that
Octa has already flagged, the way a spell checker walks from mistake to
mistake. Validation failures and outliers are included, hidden rows are
skipped, and the status bar counts them: `Problem 3 of 27`.

**Broken characters can be repaired.** A file read with the wrong character set
turns `Müller` into `MÃ¼ller`. Octa now recognises
that pattern and can undo it, both as a clean-up suggestion and as a
**Repair encoding** step in **Transform column**. The repair is verified rather
than guessed: Octa converts the text back and only accepts the result if it
comes out cleanly, so a name that merely looks unusual is left alone.

**European numbers are understood.** `1.234,56` used to arrive as text in every
reader, because it is not a number to Rust. Octa now decides per column, not per
value: a column where the grouping is consistent is promoted to numbers, and a
column that is genuinely ambiguous asks you which reading you meant, the same
way the date question already worked. A banner reports what changed and
**Dismiss** puts it back.

## Reading a very wide table

**Record view** (**F4**, or **View > Record**) shows a single row vertically as
a list of field and value pairs, which is how you read a row that has ninety
columns. Stepping to the next record follows the active filter, editing a value
works as it does in the grid, and the row you are looking at is the row selected
in the table, so both views stay in step.

## Time series

**Analyse > Time series...** does the two things people leave a spreadsheet for.

**Time buckets** group rows into equal periods, per minute up to per year, and
condense each period into one row: daily totals from a log of individual
events. **Rolling window** calculates a running result over the last N rows, a
seven day moving average for example, and keeps one value per row.

The dialog explains in a sentence what it is about to do and shows a small live
preview built from a sample, so you can see the shape of the answer before
creating the tab. On the command line the same two builders are `--resample`
and `--rolling`, and the assistant has them as tools.

## Working on a folder instead of a file

**Batch convert** (**File > Batch convert...**, or select files in the sidebar)
turns many files into one format in a single run. Every decision that can fail
is made before any file is written: output names, two inputs that would collide
on the same name, outputs that already exist, and a target format Octa cannot
write. One bad file does not abort the run, and the report at the end says what
happened to each. Also `--batch-convert` on the command line.

**Schema drift** (**File > Schema drift...**) answers the question a folder of
data parts eventually raises: do these files still agree about their columns?
Files are grouped by their exact set of columns rather than compared in pairs,
so the answer reads as "497 files look like this, 3 look like that", with the
largest group first. On the command line `--schema-drift` exits with an error
code when a folder has drifted, so a build can fail on it.

**Harmonise schemas** (**File > Harmonise schemas...**) is the repair for what
drift finds. It writes a new folder in which every file has the same columns in
the same order, and never touches the originals. A file whose values would not
survive a conversion is refused rather than written with blanks in place of
them: a folder of quietly emptied cells looks clean and is not.

## Joining tables that do not quite match

**Fuzzy join** joins on "similar to" instead of "equals", for the case where one
table says `Mueller GmbH` and the other says `Mueller Gmbh.`. Each row keeps its
single best partner, and the result carries the match score alongside a flag for
the rows where the runner up was almost as good, so a doubtful match is visible
rather than buried. Works across any number of tables, one step at a time, and
is available as `--fuzzy-join` and as an assistant tool.

**Join key finder** (**Analyse**) looks at two or more tables and ranks the
column pairs that would actually join, which saves opening both schemas and
guessing. **Use in Join** carries the answer straight into the Join dialog.

**Join diagnostics** (**Analyse**) explains a join that returned far fewer rows
than expected. It counts the keys on each side, shows examples of the values
that found no partner, and names the one change that would help: trimming
spaces, ignoring case, or dropping leading zeros. A change is only suggested
when it genuinely improves the match, so an empty answer is a real answer.

## Comparing against something that is not a file

**Compare with a database or cloud object.** The compare dialog can now put a
live database table or an object in cloud storage on the other side, instead of
a second file. Nothing new does the comparing: the same key based comparison
Octa already used simply gets its second table from elsewhere. The command line
spells it `--diff --diff-db`, and the assistant has it too.

**Cloud objects work as paths.** Any command line action that takes a file now
takes `s3://`, `az://` or `gs://` in its place, with no flag of its own. Octa
finds the credentials in the connection you saved, or falls back to whatever the
machine already has configured.

## Asking in plain English

**Ask** in the search bar turns a sentence into a filter. Type "orders over
1000 from last March" and Octa applies the conditions and shows them as
removable chips above the table. This needed a new kind of filter, since the
old one could only pick values from a list and not express "greater than".

**Ask SQL** in the SQL panel is its sibling: describe what you want and the
query is written into the editor at your cursor. It is never run for you. You
read it and press **Run**. Anything that is not a single SELECT is refused
outright, and a failed request applies nothing rather than half a query. Both
boxes send exactly one request to your configured model, with no tools and no
agent loop behind them.

## Files Octa writes

**Parquet is compressed now.** Octa used to write Parquet uncompressed, which
on a 300,000 row test file came out only 1.6 times smaller than the same data
as CSV, where zstd reached 5.4 times. The new default is **zstd**, costing about
1% more time to write and 3% more to read. The file name does not change: the
codec lives inside the Parquet file, so every reader opens it as before. All
five codecs remain in **Settings > Files > Write options**, and `--compression`
and `--row-group-size` still override per run.

**A `.tsv` is always tab separated.** A saved delimiter of, say, a semicolon
used to be written into TSV files as well. The delimiter setting is a preference
about CSV; a TSV keeps its tab whatever it says.

**The command line follows those settings too.** Previously `octa --convert`
used the built in defaults, so the terminal and the app could write different
files from the same source on the same machine. A machine with no settings file,
a container or a build agent, still gets the built in defaults.

**Excel keeps its colours.** Saving to `.xlsx` can now carry marks, conditional
formatting, frozen columns and per column number formats into the workbook.
Where Excel's own rules would colour more cells than Octa does, and they do
differ on case and on comparing text, Octa paints the cells directly instead, so
the saved workbook looks like the screen. The switch is off by default, and a
tab that has any of the four asks once when you save.

**File internals** (**Analyse > File internals...**) reports the physical shape
of a file rather than its data: row groups, compression, encodings, statistics
and which tool wrote it, plus a few plain remarks such as "the row groups are
very small". Also `--describe --deep` and available to the assistant.

## An HTML report

**File > Report...** writes one self contained HTML file with statistics,
distribution charts, top values and a correlation matrix. No JavaScript, nothing
fetched when it is opened, so it can be mailed to somebody. It invents no
analysis: every number and every picture comes from a view Octa already has, and
a report built while a filter is active describes only the rows you can see.
Also `--report` on the command line and a tool for the assistant.

## Updates and release notes

Octa checks once at launch whether a newer release exists, and shows the release
notes in a window when there is something to read. The notes for the version you
are running are shown once as well, so an upgrade announces itself instead of
waiting for the next release to exist. Both behaviours are checkboxes in
**Settings > Updates** and can be turned off, and the window has a
"do not show this again" of its own. A failed check stays quiet, because a
flaky network should not nag at every launch.

## Smaller things

- **Signing in with a browser can be cancelled**, and gives up after five
  minutes. A provider that refuses before showing the consent screen used to
  leave the button spinning with no way back.
- **Error messages can be selected and copied.** An error that cannot be quoted
  is hard to report.
- **Union can ignore case** when matching column names, in the dialog, on the
  command line and in the assistant.
- **Date and time calculation converts between time zones**, with both ends of
  a daylight saving change reported as empty rather than guessed.
- **A security fix in the SQL Server driver.** Octa's driver was the last
  component using an end of life TLS stack, which meant the binary carried two
  complete copies of it. It now runs on a patched fork with the current one, and
  the old stack is gone from the build.
- **Documentation** for all of the above, in the in app help and on the
  documentation site.
