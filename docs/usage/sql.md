# SQL Panel

Every tabular file you open in Octa is queryable via SQL. The active
table is exposed to an in-memory **DuckDB** connection as a temp table
called `data`. Press **Ctrl+Enter** in the editor and your query runs
against the loaded rows.

<!-- SCREENSHOT: sql-view.png: SQL panel docked at the bottom of the window. Editor on top with a multi-line SELECT query, result table below showing a few rows. Line numbers in the editor gutter, autocomplete chip row visible under the editor. -->
![SQL panel](../assets/screenshots/sql-view.png){ .screenshot-placeholder }

## Opening the SQL panel

Three ways:

1. **Analyse → SQL** in the toolbar (visible when the active tab is
   on a tabular file in Table view).
2. The [`ToggleSqlPanel`](../reference/shortcuts.md#view) shortcut
   (default <kbd>Ctrl</kbd>+<kbd>J</kbd>).
3. Auto-open on file load via
   [**Settings → SQL → Open SQL panel by default**](../reference/settings.md#sql).

The panel docks to the **bottom** by default. Change the side under
[**Settings → SQL → Panel position**](../reference/settings.md#sql)
(Bottom, Top, Left, or Right). The SQL panel is independent of the
[Chart](chart.md) tab; the **Analyse** menu also opens a chart in a
new tab, and the two features can be used together.

## Writing a query

The editor is a multi-line `TextEdit` with:

- **Line numbers** in a left gutter (greyed out, monospace).
- **Monospace** code throughout, defaulting to **JetBrains Mono**
  bundled with Octa. Switch to system monospace or match-UI font
  under
  [**Settings → SQL → Editor font**](../reference/settings.md#sql).
- **Right-click menu** for Copy

### Autocomplete

When the caret sits at the end of a word token, Octa shows a row of
chip-style suggestions beneath the editor, listing matching column
names and SQL keywords. Click a chip to insert, or drive the popup from
the keyboard: **Up / Down** move the highlight, **Enter** or **Tab**
accept the highlighted suggestion, **Esc** dismisses it. These keys are
only intercepted while the popup is open, so with no suggestions
showing Enter and the arrows behave normally. Disable under
[**Settings → SQL → Autocomplete**](../reference/settings.md#sql).

The editor also takes keyboard focus the moment the panel opens, so you
can start typing immediately without clicking into it first.

## Running a query

- **Ctrl+Enter** runs the entire query.
- A **Run** button in the toolbar does the same as Ctrl+Enter.
- A **Clear** button empties the editor.

Each tab owns a **persistent DuckDB workspace**: added tables and
attached databases survive across runs and are dropped when the tab
closes. See [The workspace](#the-workspace) below.

## The workspace

<!-- SCREENSHOT: sql-workspace-attachments.png: The SQL panel's Workspace section expanded. The table list shows "data (3 rows)" plus two attached connections "post_test [Postgres]" and "mariadb_test [MySQL]", one expanded to its schemas/tables. Right of the Inspector the "Attached connections" box lists both aliases with an Insert button each. In the editor below, a UNION ALL query across post_test.public.people and mariadb_test.admin.people. -->
![SQL workspace with two attached connections](../assets/screenshots/sql-workspace-attachments.png){ .screenshot-placeholder }

The collapsible **Workspace** section above the editor lists everything
your queries can reach:

- **`data`** - the active table. Queries see a snapshot taken when the
  workspace was built; after editing cells in the table view, click
  **refresh** next to `data` to push the edits in.
- **+ Add table...** loads additional files (any readable format) as
  extra tables for cross-file JOINs.
- **Attach database...** ATTACHes a DuckDB or SQLite *file*; its inner
  tables are addressed as `alias.schema.table`.
- **Attach connection** ATTACHes a saved
  [live database connection](database-connections.md) read-only
  (PostgreSQL / MySQL natively via DuckDB extensions; SQL Server,
  Oracle and the warehouses have no DuckDB extension, so their tables
  are imported as plain workspace tables, and their menu entry opens
  into the tree so you can pick how much to import: see [Picking what
  to attach](#picking-what-to-attach)). For a native ATTACH the
  **alias** you use in SQL is
  the connection name lowercased with spaces and punctuation as `_`
  ("Post-Test" becomes `post_test`). You never have to guess it: the
  **Attached connections** box next to the Inspector lists every alias
  with a one-click example query, and clicking any attached table in
  the tree offers **Copy / Insert / Run** for its qualified name.
- **Attach cloud** picks objects out of a saved
  [cloud connection](cloud-storage.md) and registers each as a
  workspace table. See [Cloud objects as workspace
  tables](#cloud-objects-as-workspace-tables) below.

### Picking what to attach

PostgreSQL, MySQL and Redshift attach natively: one click takes the
whole server, nothing is copied, and every table stays queryable as
`alias.schema.table`.

Every other engine (SQL Server, Oracle, ClickHouse, Exasol, Snowflake,
Databricks, BigQuery, Trino) has no DuckDB extension, so an attach
*fetches* the tables it covers. Their entry under **Attach connection**
therefore opens into the server's tree instead of attaching straight
away, and you say how much you want:

- a **single table** - the fastest and the usual answer,
- a **schema**, which brings in the tables under it, or
- **Attach everything here**, which takes the whole level you are
  standing on.

Trino, Snowflake, Databricks and BigQuery put a **catalog** above the
schema, so their tree starts one level higher: catalog, then schema,
then table. "The schemas of this server" is not a question those
engines can answer, so their root has no **Attach everything here**;
pick a catalog first.

An imported table is named after **itself**: pick `orders` out of a
Databricks catalog and you write `SELECT * FROM orders`. It joins the
workspace as an ordinary table (not as an attachment, since nothing
stays connected once the rows are copied), and where it came from is on
the row: hover it to see `warehouse main.sales.orders`. A name the
workspace already uses takes a `_2` suffix.

**Double-click a table's name to rename it**, so a `FROM` clause says
whatever you want it to. Names are lowercased with spaces and
punctuation turned into `_`, the same as everywhere else in the
workspace, and a name another table holds is refused rather than
silently overwriting it. `data` is the one name you cannot change: the
refresh button and the edit path both address the active tab by it.

An import is capped at 60 tables per attach and at the **maximum rows
loaded on open** setting per table. Over that cap nothing is attached
at all and the message says so: no table is ever silently left out of
an attachment. Drill in one level further, or query the server directly
with **Run on server** instead.

### Cloud objects as workspace tables

**Attach cloud** lists your saved [cloud connections](cloud-storage.md)
and opens a picker on the one you choose. Browse the folders, tick the
objects you want, and **Add to workspace** downloads each and registers
it as a table named after the file. Only files a reader can open are
offered, so you cannot tick something that would fail on download.

Ticking several objects gives you several tables. **Combine the picked
objects into one table** unions them into a single table instead, and it
starts off on purpose: a union reconciles differing schemas, and Octa
does not do that to your data unless you ask. A column missing from one
file comes back empty for that file's rows, exactly as in
[Union tables](union-tables.md).

Clicking a table in the list opens it in the **Inspector**: columns,
types, and a sample of rows.

The workspace also works with **no table open at all**: open the panel
via **Analyse > SQL** on an empty tab, attach your connections, and
query the servers directly (cross-server JOINs and UNIONs included);
there is simply no `data` table until you open a file.

## History and snippets

The SQL toolbar has two ways to reuse queries:

- **History** is a dropdown listing the queries you have actually run, most
  recent first, each with how long it took and how many rows it returned. Pick
  one to load it back into the editor, or use **Clear history** at the bottom to
  forget the lot.

    History is **scoped and kept between sessions**: a database tab records
    against its connection, a file-backed workspace against its file, so the
    queries you ran on production do not turn up while you are poking at a CSV.
    It lives in `sql_history.json` in the
    [config directory](../reference/settings.md).

    **Settings -> Databases -> Query history** controls it: *Keep the queries I
    run* is on by default and keeps the last **20** per connection (0 keeps them
    all). Turning it off stops recording **and deletes what was kept**, because
    a query can carry values out of your data and a switch that leaves the old
    file behind would be a poor kind of off. Re-running a query moves it back to
    the top rather than adding a second copy.
- **Snippets** opens a **manager window** for a persistent, named library
  of queries. **Save current query as snippet...** stores the editor
  content under a name and an optional description; each saved snippet has
  **Insert** (load it into the editor) and **x** (delete). The window has
  the usual minimise / maximise / close controls and is resizable.
  Snippets are stored in `sql_snippets.json` in the
  [config directory](../reference/settings.md), so they survive restarts
  and are shared across all tabs.

## Ask

Under the toolbar there is an **Ask** box, on a row of its own with the Ask
button and the assistant picker. Type what you want in plain words
("revenue per country, biggest first") and Octa writes the SQL into the editor
at your cursor. The rest of the editor is left alone, so you can ask for one
piece of a query you are already writing.

The box starts one line tall and **grows as you type**, so a long question
stays readable instead of scrolling sideways out of sight. **Enter** sends the
question and **Shift+Enter** starts a new line.

Beside the box, a dropdown picks **which assistant answers**, the same control
the search bar's Ask has. It is per tab and starts on whatever the chat panel
is set to, so you can send one question to a bigger model without changing the
panel. It only appears when more than one profile is configured, since with one
there is no choice to make.

The query is **never run for you**. Read it, change it if you like, then
press Run. Only a single SELECT is ever produced: a reply containing a
second statement, or anything that is not a SELECT (or a leading `WITH`),
is rejected and nothing is inserted.

Ask sends the active table's column names, their types and the row count
to the chat profile configured under
**Settings > Chat / Assistant**. It does not send the data itself.

It also sends the **workspace**: every table registered from another tab
and every attachment's tables, with their columns, so it can write a join
across them and call each one by its real name. The list is capped at 12
tables and 40 columns apiece to keep the request small; attachment tables
come from the cached listing, so no extra query goes to the server.

When the panel is set to run on a server, the query is written in that
database's dialect against the real `schema.table` name instead of
`data`, and the row count is the table's own, not the size of the page
currently on screen.

Ask is greyed out when no chat profile is set up, or when there is
nothing for the prompt to describe: no columns on the tab **and** no
registered or attached workspace tables. An empty tab with a database
attached is therefore fine, which is exactly the tab the panel exists
for. Hover it to see which case applies.

## What's available

DuckDB's full SQL surface, including:

- Window functions: `ROW_NUMBER()`, `RANK()`, `LAG()`, etc.
- Aggregations: `SUM`, `AVG`, `COUNT`, `MEDIAN`, percentiles, etc.
- JSON functions: `json_extract`, `unnest`, …
- Date/time functions, string functions, regex functions.
- CTEs (`WITH ... AS (...)`), subqueries, correlated subqueries.
- `PIVOT` / `UNPIVOT`.
- `DESCRIBE data` to see the column types DuckDB sees.

The placeholder query shown when the editor is empty is
`SELECT * FROM data LIMIT {settings_default}` (the default row
limit is configurable under
[**Settings → SQL → Default row limit**](../reference/settings.md#sql)).
This is only a hint; your editor field is actually empty, so type to
replace.

## Result rendering

Results render in a table below the editor, with a **row counter**
directly above the grid, followed by how long the query took in
brackets (`1234 result rows (84 ms)`, switching to seconds past one
second). The timing covers the query itself, and for a query run on a
live connection it covers the round trip to the server. A query that
fails is timed too, so a slow statement that ends in an error still
tells you where the minute went. The counter is display-only: it is
never a column of the result and never lands in an export. The result table is a
separate `egui_extras::TableBuilder` from the main
[Table view](table-view.md) (no edit overlay, no row selection
beyond click-to-select-text).

Results honour the same **initial-load row cap** as file opens
([**Settings → Performance**](../reference/settings.md#performance),
default 5,000,000): a SELECT that would return more rows stops there
instead of exhausting memory, and the row counter says so
("row cap reached, result truncated"). This applies to local DuckDB
queries and to queries run on a live database connection alike. Raise
the cap, or narrow the query, to see more.

Errors render in **red** below the editor.

## CREATE TABLE opens a new tab

A `CREATE TABLE` or `CREATE VIEW` statement turns into a **new tab** named
after the table, with the declared columns and types:

```sql
CREATE TABLE people (id INTEGER, name VARCHAR, born DATE)
```

gives an empty grid to type into, and

```sql
CREATE TABLE top AS SELECT * FROM data ORDER BY score DESC LIMIT 10
```

gives a tab holding those rows. Either way the tab you ran it from is
untouched, so this works from an empty tab with nothing open (the SQL
panel opens there too), and it is not blocked by read-only mode: a new tab
is not an edit. Save the new tab like any other table.

The table is handed over rather than kept: after the statement, `people`
is no longer queryable from the original panel. Switch to the new tab and
it is `data` there. `CREATE OR REPLACE` of a table that already exists
(`data` included) is an ordinary mutation of that table, and a `CREATE`
inside an attached database is left to that database (attachments are
read-only, so DuckDB refuses it).

On the command line, `octa --sql file -q "CREATE TABLE ..."` prints the
created table; the `run_sql` MCP tool returns it as `result` with
`"created": "<name>"`.

## Mutations

`INSERT` / `UPDATE` / `DELETE` queries run via `conn.execute()`
instead of `conn.query()`. After a mutation, Octa re-selects the
full `data` table and replaces the **base table** in the active
tab, so the mutation's effect is visible immediately.

To make the effect easy to spot, Octa **briefly highlights the
changed cells and any new rows in green** after a mutation. Toggle
this and set its duration under
[**Settings → SQL**](../reference/settings.md#sql) (**Highlight SQL
changes** / **Highlight duration**, on by default, 4 seconds). The
highlight is a temporary display mark and clears itself.

!!! warning "Mutations don't persist back to disk by default"

    A mutation changes the **in-memory** table only, so it is lost
    when you close Octa unless you also save the file via
    **File → Save**.

    For files Octa supports
    [writing](saving.md) (CSV, Parquet, SQLite, …), saving after
    a mutation persists the change. For
    [read-only formats](saving.md#read-only-formats) (SAS,
    HDF5, …) the change is in-memory-only, though you can
    **Save As** to a writable format to export it.

## Exporting results

The toolbar's **Export…** button (and the
[**Ctrl+Shift+E** shortcut](../reference/shortcuts.md#sql-panel)) saves the current SQL result as a separate file. The
dialog accepts any writable format Octa supports: Parquet, CSV,
JSON, SQLite, etc.

## Examples

```sql
-- Count rows per category
SELECT category, COUNT(*) AS n
FROM data
GROUP BY category
ORDER BY n DESC;

-- First / last per user
SELECT user_id,
       MIN(timestamp) AS first_seen,
       MAX(timestamp) AS last_seen
FROM data
GROUP BY user_id;

-- Rows containing JSON
SELECT id, json_extract(payload, '$.user.email') AS email
FROM data
WHERE payload IS NOT NULL;

-- Window function: rolling 7-day count
SELECT date,
       COUNT(*) OVER (
         ORDER BY date
         RANGE BETWEEN INTERVAL 6 DAY PRECEDING AND CURRENT ROW
       ) AS rolling_7d
FROM data;

-- DESCRIBE for schema discovery
DESCRIBE data;
```

## Limitations

- **One table per session.** Only `data` is registered, so there is
  no way to JOIN across two open tabs from the GUI yet (use `octa --sql`
  with two files, or copy-paste the relevant data).
- **No DDL persistence.** `CREATE TABLE other AS SELECT ...`
  succeeds but the new table dies with the connection on the next
  Ctrl+Enter.
- **No extensions yet.** DuckDB has powerful extensions
  (`spatial`, `postgres_scanner`, `sqlite_scanner`, etc.), but
  they are not auto-loaded by the SQL panel.

For multi-file analysis the CLI's
[`octa --sql FILE -q 'SELECT ...'`](../cli/sql.md) is a good
companion: it spins up a fresh DuckDB and you can layer ATTACH /
COPY however you want.

## See also

- [`octa --sql`](../cli/sql.md) is the CLI form of this panel.
- [Settings → SQL](../reference/settings.md#sql) covers
  autocomplete, panel position, default row limit, and editor font.
- [Search & Filter](search-and-filter.md) covers value-based
  filtering that does not need SQL.
- [Chart](chart.md) opens the active table in a new chart tab from
  the same **Analyse** dropdown.
