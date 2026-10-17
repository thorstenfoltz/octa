A release about where files live, how big they can get, and an Octa that no
longer stops while it works. SFTP and WebDAV servers, Nextcloud included, join
S3, Azure and Google Cloud Storage in the sidebar, and a file opened from
outside Octa lands as a tab in the Octa you already have open. Saving,
SQL queries, attaching databases and reading large files now run in the
background, each with a Cancel, so the window keeps responding on millions of
rows. Large-file mode answers SQL, filters and Save as for the whole file, not
the page on screen. Save as asks before it writes, Octa can keep earlier
versions of your files, and a database schema exports to a folder of files in
one go. A query can be explained before it runs, Compare accepts a tolerance,
text can sort naturally, a binary cell can be opened and looked at, and a
notebook shows its plots. A table splits into files of a set size, a file can
be saved behind a password, a database save no longer overwrites what someone
else changed in the meantime, and two or more folders can be compared file by
file. A browser sign-in to a database now lasts across restarts, and Mac users
install Octa with one line.

## Breaking changes

These change what an existing setup does. None of them touches your files.

- **Dates are no longer detected when a file opens.** A text column holding
  only dates used to become a date column on its own; on a file of millions of
  rows that check froze the window for over a minute. Turn it back on under
  **Settings > File-Specific > Detect dates on load**, or use
  **Columns > Detect dates** on the table that needs it.
- **A flag that belongs to another action is refused.** On the command line,
  `octa --head data.csv --rules rules.toml` used to run `--head` and silently
  ignore `--rules`. It now stops with an error naming the flag, so a script
  that passed such a flag fails until the flag is removed.
- **A read-only database connection only runs reading statements**: those
  starting with `SELECT`, `WITH`, `SHOW`, `DESCRIBE`, `EXPLAIN`, `VALUES`,
  `TABLE`, `USE`, `LIST`, `HELP` or `EXISTS`. A procedure call or any other
  statement on a read-only connection is now refused; allow writes on the
  connection if it needs them.
- **SQL Server `datetimeoffset`** reads as text with its offset instead of a
  timestamp without it. Tick **Read datetimeoffset as a timestamp (UTC)** on
  the connection to get the old behaviour back.
- **Split into files numbers its parts per folder**: every folder starts at
  `part-0001`, where the numbers used to run on across folders.
- **In the SQL panel, `data` is an ordinary table**, no longer a temporary
  one. A query that named it `temp.data` has to say `data`.
- **A file opened from outside Octa joins the Octa already open** instead of
  starting a second one. **Settings > Window** switches that off.

## What's new

### SFTP and WebDAV servers

**Settings > Cloud storage > Add connection** offers **SFTP** and **WebDAV /
Nextcloud**. Browse, open, save back, copy, move, delete and Union work as
they do for a bucket.

- SFTP signs in with your SSH agent, a key file or a password, and checks the
  server against `~/.ssh/known_hosts` like the database tunnel does. A save
  goes to a hidden file next to the target first, so a dropped connection
  never leaves a half-written file under the real name.
- WebDAV takes the server address, your user name and an app password.
- `sftp://` and `davs://` paths work on the command line and over MCP, and
  `--add-connection` can create both kinds.

### Files open in the Octa you already have

Double-clicking a file, or `octa sales.csv` in a terminal, adds a tab to the
open Octa instead of starting another one. Starting Octa without a file
still opens a new window. **Settings > Window** turns it off.

### A notification when a long task is done

When a task that ran at least 5 seconds (your choice) finishes while you are
in another program, Octa shows a system notification: a query, a download,
a split, an export, a search, a comparison, any task that runs in the
background. It is on by default; **Settings > Window > Notify me when a long
task finishes** turns it off, and **Also when Octa is in front** sends it
even while you are looking at Octa. On KDE it appears in Plasma's
notifications like any other app's.

### The window keeps responding while Octa works

On a table of millions of rows these used to stop the window until they were
done. They now run in the background, the status bar says what is running,
and the tab stays read-only only where it has to:

- **Saving**, in every format, backups and earlier versions included. Only
  SQLite, DuckDB and GeoPackage files, which are changed in place, still save
  while the window waits. Saving from the close or exit question closes the
  tab or Octa once the file is written.
- **SQL queries** on the local engine, with a **Cancel** beside the spinner.
  A query that only reads leaves the tab editable.
- **Copying the table into SQL**, and your other open tabs, only when they
  changed since the last copy.
- **Attaching a database or a file** to the SQL workspace, and **+ Add
  table...** and **Attach cloud**, with progress and **Cancel** in the status
  bar. An attach or import is all or nothing: a cancelled one takes back what
  it already brought in.
- **Explain** on a table that changed since the last Run copies it first.
- **The answers to the date and number questions** when a column is
  ambiguous.
- **Opening a large file and every page of it**.

### Large-file mode covers the whole file

A file bigger than **Settings > Performance > Large-file mode above** (2 GB)
stays on disk and shows a page at a time. What used to look only at that page
now covers every row:

- The SQL panel's `data` is the whole file, read where it lies:
  `SELECT state, count(*) FROM data GROUP BY state` counts every row.
- **Column Filter**, **Filter by value** and the **Ask** box filter the whole
  file, and the value lists count it.
- **Save as** writes the whole file, or the filtered rows in the order the
  view is sorted by, to CSV, TSV, Parquet, JSON or JSON Lines.

Large-file mode now starts on a file's size alone. A small Parquet file that
holds many rows opens the ordinary way.

### Detect dates on demand

**Columns > Detect dates** turns text columns that hold only dates into date
columns, in the background, and one **Ctrl+Z** undoes it.

### Save as asks before it writes

After you pick a file, **Save as** asks what applies, in one small window.
With a search or filter on: **all rows**, or **only the rows you see** (until
now it always wrote only the visible ones, without a word). Then whether to
**open the new file** afterwards, on by default. Both questions can be switched
off under **Settings > Files**, each with the answer to use instead.

A Save as to a `.csv` also asks how to separate the values, in plain words
(comma, semicolon for spreadsheet programs in most of Europe, tab, vertical
bar), how decimal numbers should look, and whether spreadsheet programs should
show accents correctly. A preview shows the first lines as they will be
written.

### Earlier versions of a file

Off by default. Turn on **Settings > Files > Keep earlier versions** and every
save over a file first keeps the version it replaces, in Octa's own folder.
**File > Earlier versions...** opens one, compares it with the tab, or restores
it (keeping the current file as a version first, so a restore can be undone).
You choose how many versions per file and how much space in all, either, or
no limit.

### Fix the header row after opening

Exports with a title block above the real column names, a header split over
two rows, or no header at all can now be fixed after opening. Right-click the
row that holds the names and pick **Use as header row**; select two rows first
to combine a merged header into names like `2025 Q1`. When the names look like
placeholders, a banner suggests the row. Column types are detected again
afterwards, Ctrl+Z undoes it in one step, and Recipes replay it on next
month's file. **Edit > First row is header** now works the same way and can be
undone.

CSV files with a title line no longer open one column wide.

### Delta and Iceberg history

**File > Earlier versions** on a Delta Lake or Apache Iceberg table lists the
table's own history: version, time, what happened and how many rows. Open
any version in a tab, compare it with the current table, or see what one
change added and removed. The comparison runs over both complete versions,
however large.

### Export database tables to files

Right-click a schema in the Databases sidebar and pick **Export tables...**:
tick the tables, choose a folder and Parquet, CSV, TSV or JSON Lines, and each
table becomes one file. Tables are read page by page, so their size does not
matter, a file only appears once it is complete, and one failed table does not
stop the rest. Also `octa --db NAME --db-export SCHEMA --out-dir DIR` and the
MCP tool `export_db_tables`.

### Sign in to a database from the sidebar, once

When an Azure AD, GCP IAM, AWS IAM Identity Center, Databricks or Snowflake
connection in the **Databases** sidebar has no valid sign-in, the error comes
with a **Sign in with browser** button right under it, and the connection
lists again once you are signed in. The sign-in is kept in your system
keyring until the token expires, so it survives a restart of Octa, and the
command line and the MCP server use it too. **Sign out** forgets it.

### Attach any number of tables

Attaching SQL Server, Oracle or a warehouse to the SQL workspace copies its
tables in. The limit of 60 tables per attach is gone, since the copy now runs
in the background with progress and **Cancel**.

### Open and save SQL files

The SQL editor's **File** menu opens a `.sql` file into the editor, and saves
the editor back to it or to a new file. The editor shows the file's name and a
`*` while there are unsaved changes, and **Ctrl+S** in the editor saves the
SQL file. Line endings are kept, and **Keep earlier versions** applies.

### Explain a query

The **Explain** button beside **Run** shows how a query would be carried out,
without running it. It works on the local engine, PostgreSQL, MySQL, MariaDB,
Databricks and BigQuery. Three views are offered: the steps in plain words
first, numbered in the order the database works through them, each with the
condition it keeps rows by, how many rows a limit keeps or what a group or
sort goes by; then a table, then the server's own output. BigQuery shows how
many bytes the query would read, never a price. Other engines show the button
disabled and say why. `--explain` on `--sql` and `--db-query`, and
`explain: true` on `run_sql` and `query_db`, do the same for scripts and
agents.

### Compare with a tolerance

Compare can now treat numbers and times that are close enough as equal: a
difference in absolute terms or as a percentage for numbers, a number of
seconds for times. It can also ignore case, ignore spaces, and leave columns
out. Cells that differ only within the tolerance are counted separately. It
is on the compare view, in Compare with database, as `--diff-tolerance` and
its companions on `--diff`, and as `tolerance` on `diff_tables`.

### Two new validation rules

Compare two columns (`end` at least `start`) and check a column against one
of these values, picked by ticking them. Both work in the rules dialog, in
rules files, with `--check` and with `check_rules`, and the report names the
values involved.

### Natural sort

With **Settings > Table View > Sort text** set to **Natural**, `file2` sorts
before `file10`; the default stays letter by letter. Recipes remember it.
`--sort-by`, `--sort-desc` and `--natural` on `--head` and `--convert`, and
`sort` and `natural` on `read_table` and `convert`, do the same on the command
line and over MCP.

### See what a binary cell holds

Right-click a cell and choose **View contents...** to see a picture, the
text of a PDF, the files in a zip, the contents of a gzip, formatted JSON or
a hex dump. **Save to file** and **Open in default app** are in the same
window.

### Pictures in notebooks

The Notebook view draws the pictures in a cell's output, plots included, in
the order the notebook holds them: PNG, JPEG, GIF, WebP and SVG, at their own
size on a white background as in Jupyter. Click one to open it in the binary
viewer to see it full size, save it or open it in another program. In the
Table view, on the command line and over MCP, a picture that comes with no
text reads as a placeholder like `[image/png 640x480]` instead of nothing.
The binary viewer shows SVG cells as pictures too.

### Split a table into files of a set size

**Data > Split into files...** splits by the values of a column, as before,
and now also into files of at most a number of rows, files of at most a size
(for an upload limit, say), or both: a file per city, each cut into parts of
at most 100,000 rows. The dialog previews the file names before anything is
written, and a long split can be cancelled. On the command line it is
`--split-rows` and `--split-size`, and the `partition_table` tool takes the
same options.

### Save with a password

**File > Save with password...** saves the tab as a zip file encrypted with
AES-256, and the dialog says so: for `report.csv` the file on disk is
`report.csv.zip`. Octa opens it on any system; 7-Zip (Windows) and Keka (Mac)
do too, while the zip tools built into Windows and Mac cannot. **Remember**
keeps the passphrase in this computer's keychain. Batch convert can protect
every file it writes, and the command line takes the passphrase from an
environment variable with `--password-env`.

A tab opened from a password-protected zip now saves back into that zip, and
the unencrypted copy Octa read it from is removed when the tab closes.

### Database saves keep other people's changes

Until now, saving a database table wrote your rows over whatever was on the
server, so a colleague's change made after you opened the table was lost.
Now Octa reads the rows you changed again just before it writes. Their change
in a cell you did not touch is kept along with yours, and the tab shows it
afterwards. When you both changed the same cell, or one of you deleted a row
the other edited, nothing is saved: a window shows the value from when you
opened the table, the value on the server now and yours, and you choose what
to keep. Postgres, MySQL, Oracle and SQL Server lock those rows until the
save is done.

### Compare folders

**Analyse > Compare folders...** compares two or more folders file by file,
on this computer, in a cloud bucket, or a mix: which files are the same,
which differ and which are missing where. Copies of the same size are
compared by their contents. Right-click a row to open two copies side by
side in Compare. On the command line it is `--compare-folders`, which exits
with 1 when anything differs, and the `compare_folders` tool answers the
same question for an assistant.

### Install on a Mac with one line

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash
```

puts `Octa.app` into `~/Applications` and `octa` into `~/.local/bin`, with no
password and no Gatekeeper warning. The installation guide now describes the
**Open Anyway** steps that macOS 15 asks for when the app comes from a browser
download.

### Smaller additions

- **Move a column one step** with the up and down arrows on each row of the
  Column navigator.
- **The schema export preview** is coloured like the Raw view.
- **Windows icons**: the Store version's taskbar icon no longer sits on a
  coloured square, and the zip and installer versions' taskbar icon follows
  the colour picked in **Settings > Appearance**.
- **Microsoft Store copies** get **Help > Updates in the Microsoft Store...**,
  which opens Octa's page in the Store with its Update button.

## Changed

- **Picking a query from the SQL History menu or inserting a snippet** adds
  it below what the editor already holds, marked so **Run** runs just that
  query. Both used to replace the editor text without asking. **Settings >
  SQL > Query history > A picked query replaces the editor text** brings the
  old way back.
- **Right-click > Delete column** deletes the column at once instead of
  opening the column picker; **Ctrl+Z** brings it back. With several columns
  selected, it reads **Delete selected columns** and deletes them all, and
  one **Ctrl+Z** restores them all. **Columns > Delete columns...** still
  opens the picker.
- **Transpose** in the Analyse menu lost its `...`: it runs at once and asks
  nothing, so the dots promised a dialog that never came.
- **SQL Server `MONEY`** columns are numbers.
- **The memory figure in a tab name's tooltip** is worked out from a sample of
  at most 10,000 rows, so hovering a tab costs nothing at any size.

## Fixes

- **Saving a large-file tab could overwrite the file with one page.** A plain
  Save on a tab in large-file mode, confirmed through the partial-table
  question, wrote the 2,000 rows on screen over the original file. Save is now
  refused there; **Save as** writes the whole file.
- **Save as wrote part of a table without saying so.** On a table still
  loading its rows, Save as wrote only the rows loaded so far, 5 million of 18
  million in one report, and announced success. It now asks first, like Save.
- **Opening any file read it several times over.** A check for password
  protection made a large CSV be read end to end about eight times while it
  opened, enough to run a computer out of memory. It now looks at the first
  four bytes.
- **Line breaks in cells froze large tables.** With cell line breaks on, Octa
  measured every row's height at once, which at millions of rows took close to
  a minute and could run out of memory. Only the rows on screen are measured
  now.
- **Rows loaded while scrolling were incomplete or slow.**
  - Each new batch of a CSV re-read the file from the start, so loading slowed
    down the further you scrolled; it now continues where it stopped.
  - Rows that arrived later did not get the load's date, number and
    whitespace conversions; now they do.
  - A batch arriving at the wrong moment could be lost.
  - Rows dropped to save memory now follow the Settings row limit.
- **Large-file mode asked about small files.** A 35 MB Parquet saved from a
  5 GB CSV was treated as large because of its row count.
- **Snowflake browser sign-in opened the browser on every connect.** The
  token is now kept until it expires.
- **SQL editor buttons squeezed together in a narrow panel**: Export, Write
  result to DB and Open result as tab broke their labels letter by letter.
  They now move to the next line whole.
- **SQL suggestions opened while marking text.** Once you had typed, the
  list came up whenever the caret landed after a name, marking column names
  included, and then took **Enter** or **Tab** as a pick. It now shows only
  while you type a word; a click, an arrow key or a selection closes it.
- **Esc in the SQL suggestion list** threw you out of the editor, and Enter
  then took a suggestion instead of starting a new line. Esc now closes the
  list and leaves the caret where it was, so the next Enter is a line break.
- **Notebook outputs set to Beside were off screen.** Since cells became
  editable, a cell's source took the whole width and pushed its output past
  the right edge, reachable only by scrolling sideways. A cell with output
  now gives its source half the width.
- **Saving a database table could change columns you never touched.** A
  table open for editing writes whole rows back, so any value Octa had read
  wrongly was written back wrongly, on the server, by an edit somewhere else
  in the row. Found and fixed against real PostgreSQL, MySQL and SQL Server
  servers:
  - PostgreSQL: `uuid`, `json`, `jsonb`, `bytea`, `interval`, `time`, `inet`,
    arrays, enums, `money` and every other type outside numbers, booleans,
    dates and timestamps read as empty and were saved as NULL. Values now
    arrive exactly as PostgreSQL prints them, `bytea` as real bytes.
  - PostgreSQL `timestamptz` moved by an hour or two on a server whose time
    zone is not UTC, as on most servers in Germany.
  - SQL Server: `DECIMAL`, `NUMERIC`, `MONEY`, `uniqueidentifier`,
    `varbinary`, `time` and `xml` read as empty and were saved as NULL.
  - MySQL: a `BLOB` or `VARBINARY` came back with every byte that is not
    text replaced, and a backslash in text was saved as an escape, so
    `C:\temp` turned into `C:`, a tab and `emp`.
  - Timestamps lost their fractions of a second on every server, so a save
    turned `12:00:00.789` into `12:00:00`.
  - SQL Server turned text outside the database's code page, such as
    Chinese or emoji, into `?`, and dropped a backslash at the end of a line
    together with the line break.
- **Writing a table to a warehouse changed text.** Save to database, the copy
  dialog and the `write_db_table` tool write text as SQL, and each engine
  reads that text by its own rules. Snowflake, ClickHouse, Databricks and
  BigQuery turned a backslash into an escape, Databricks joined `it's` into
  `its`, and BigQuery refused text with a line break or an apostrophe. Text
  now arrives exactly as written on every engine. Checked against real
  ClickHouse, Trino, Oracle, PostgreSQL, MySQL and SQL Server servers, and
  against Spark for the Databricks rules.
- **Appending to an existing table missed it when the case differed.** Octa
  quotes every name it writes, so rows headed `id` could not find a column
  the server stores as `ID`, and typing `orders` could not find `ORDERS`. On
  Oracle and Snowflake, which store unquoted names in upper case, that was
  nearly every table. Append now uses the server's own spelling of the table
  and its columns.
- **A read-only database connection could still be written to.** Octa
  decided whether a query writes by its first word alone, so a `DELETE` after
  a comment line, `SELECT 1; DELETE FROM t` on MySQL or BigQuery,
  `SELECT 1 DELETE FROM t` on SQL Server, `WITH x AS (DELETE ...) SELECT`,
  `SELECT ... INTO` and procedure calls all got through. PostgreSQL, MySQL,
  MariaDB and ClickHouse now also hold the connection read-only on the
  server, and SQL Server rolls every batch back afterwards. This covers the
  SQL panel, the command line and agents.
- **Read-only mode let edits through**: the right-click menus, **Ctrl+X** and
  **Ctrl+V**, Replace, Date/Time calculation and several Edit and Columns menu
  entries still changed the table. They are all blocked now, and the entries
  are greyed out with a note saying the tab is read-only. Sorting, filtering,
  searching, copying and colour marks still work.
- **A European CSV stays European**: a CSV with numbers like `1.234,56` was
  read as numbers but saved back as `1234.56`, which a German or French
  spreadsheet program then no longer read as a number. A plain Save now keeps
  the decimal commas, the separator and the byte order mark the file had.
- **Save as from a semicolon CSV to another format** wrote semicolon-separated
  text into the new file, whatever its extension said, so a `.parquet` or
  `.tsv` saved that way could not be opened. The format now follows the file
  name you pick.
- **Rows repeated or missing while a file was still loading**: on a CSV or
  Parquet file that loads more rows as you scroll, deleting or inserting a row
  by hand made the next batch start one row too early or too late. Batches
  now continue where the reading of the file stopped.
- **Fields not in line**: in rows that mix labels, checkboxes, buttons and
  drop-downs, the labels sat a few pixels high and the drop-downs a few pixels
  low. Every such row across the app now lines up, title bars included.
- **File > New file could only be saved with Ctrl+S**: the File menu hid
  **Save as...** for a document without columns. It shows now.
- **MCP errors name the cause**: a failed tool call reported only its last
  step, such as `query_db failed: querying 'shop'`, and dropped the server's
  own reason. Every tool now reports the whole chain.

## Under the hood

- Two new dependencies, both MIT or Apache-2.0: `russh-sftp` for SFTP (pure
  Rust, on the SSH library Octa already uses for the database tunnel) and
  `notify-rust` for notifications (on Linux it speaks to the notification
  service through the D-Bus library already in the build).
- Dependencies updated, which closes four security advisories in the TLS
  library. Three advisories in the zip library stay open until the Excel
  reader moves to its next version; they concern unpacking a zip to disk,
  which Octa never does.
- Every pull request now builds and tests Octa on macOS as well as Linux, and
  each release starts the finished Mac app once before publishing it.
- SFTP and WebDAV are tested against real servers in CI, next to the
  PostgreSQL and MySQL tests.
- The AppImage carries update information, so AppImageUpdate and launchers
  that use it can update it in place by downloading only what changed, plus
  AppStream metadata with a description, a screenshot and links for software
  catalogues.
- The documentation covers everything above in both the in-app Help and the
  website, with a new page for notifications.
