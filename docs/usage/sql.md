# SQL Panel

Every tabular file you open in Octa is queryable via SQL. The active
table is exposed to an in-memory **DuckDB** connection as a temp table
called `data`. Press **Ctrl+Enter** in the editor and your query runs
against the loaded rows.

<!-- SCREENSHOT: sql-view.png — SQL panel docked at the bottom of the window. Editor on top with a multi-line SELECT query, result table below showing a few rows. Line numbers in the editor gutter, autocomplete chip row visible under the editor. -->
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
names and SQL keywords. Click a chip to insert. Disable under
[**Settings → SQL → Autocomplete**](../reference/settings.md#sql).

## Running a query

- **Ctrl+Enter** runs the entire query.
- A **Run** button in the toolbar does the same as Ctrl+Enter.
- A **Clear** button empties the editor.

Each run creates a **fresh DuckDB connection**, so there is no
persistent SQL state between queries. Accidental mutations therefore
do not accumulate across runs; an `UPDATE` in one query does not
carry over to the next.

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

Results render in a table below the editor. The result table is a
separate `egui_extras::TableBuilder` from the main
[Table view](table-view.md) (no edit overlay, no row selection
beyond click-to-select-text).

Errors render in **red** below the editor.

## Mutations

`INSERT` / `UPDATE` / `DELETE` queries run via `conn.execute()`
instead of `conn.query()`. After a mutation, Octa re-selects the
full `data` table and replaces the **base table** in the active
tab, so the mutation's effect is visible immediately.

!!! warning "Mutations don't persist back to disk by default"

    The in-memory DuckDB connection is recreated on every Ctrl+Enter,
    so any mutation is **lost** when you close Octa unless you also
    save the file via **File → Save**.

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
