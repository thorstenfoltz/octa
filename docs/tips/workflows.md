# Tips & Recipes

Worked workflows that chain Octa's features together. Most of these
are 2-3 steps; pick the one that matches your task and follow along.

## Convert a messy CSV to a clean Parquet

When you've got a CSV with mixed types, the date inference and
type promotion happen automatically. Round-tripping through
Parquet makes them stick.

### Via the GUI

1. **Open** the CSV. Octa auto-detects the delimiter (see
   [CSV Quote / Escape](../reference/csv-quote-escape.md) if
   detection is wrong).
2. **Inspect the columns**, right-clicking a header → **Change Type**
   if a column was inferred wrong. See
   [Date Inference](../reference/date-inference.md) for how date
   columns get promoted automatically.
3. *(Optionally)* Edit a few cells via the
   [Table view](../usage/table-view.md), mark some rows with
   [Colour Marking](../usage/colour-marking.md).
4. **File → Save As…** → name it `clean.parquet`.

Octa picks the Parquet writer from the extension, applies your
type changes, and writes a properly-typed Parquet file (see
[Saving](../usage/saving.md)).

### Via the CLI

```bash
octa --convert messy.csv clean.parquet
```

Type inference still applies: CSV column types are inferred at
read time and become the Parquet schema. See
[`octa --convert`](../cli/convert.md) for the full reference.

For larger jobs:

```bash
# Run a SQL pre-process first, then convert the result
octa --sql messy.csv -q 'SELECT id, name, CAST(amount AS DOUBLE) AS amount FROM data' -f csv > clean.csv
octa --convert clean.csv clean.parquet
rm clean.csv
```

## Open a huge file without blowing memory

Octa's streaming readers (Parquet, CSV, TSV) load only the first
1M rows by default. For files with more rows than that:

1. [**Settings → Performance → Initial-load row cap**](../reference/settings.md#performance):
   raise it if you regularly work with bigger files (and have
   RAM to spare).
2. **For one-off analysis** without changing the setting, point
   DuckDB at the file directly outside of Octa, since it streams
   Parquet / CSV / JSON natively without any cap. The Octa
   [SQL panel](../usage/sql.md) and
   [`octa --sql`](../cli/sql.md) both honour the initial-load cap,
   so neither bypasses it.

## Find duplicate rows in a CSV

```sql
-- Open the file in Octa, switch to the SQL view
-- (Ctrl+J / ToggleSqlPanel), and run:
SELECT *, COUNT(*) AS n
FROM data
GROUP BY ALL
HAVING n > 1
ORDER BY n DESC;
```

`GROUP BY ALL` groups by every column; rows with `n > 1` are
duplicates. For "duplicates by some columns only":

```sql
SELECT customer_id, order_date, COUNT(*) AS n
FROM data
GROUP BY customer_id, order_date
HAVING n > 1;
```

## SQL exploratory analysis

When you don't know what's in a file yet:

```sql
-- 1. What columns + types
DESCRIBE data;

-- 2. Top-N for each categorical column
SELECT region, COUNT(*) AS n FROM data GROUP BY region ORDER BY n DESC LIMIT 10;

-- 3. Distribution of a numeric column
SELECT
  MIN(amount) AS min,
  PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY amount) AS p25,
  MEDIAN(amount) AS median,
  PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY amount) AS p75,
  MAX(amount) AS max,
  AVG(amount) AS mean
FROM data;

-- 4. Find suspicious values
SELECT * FROM data WHERE amount > 1e7 OR amount < 0;
```

## Compare two CSVs

For a quick line-by-line text diff, open one CSV → **View →
Compare with…** → pick the other. The
[Compare view](../usage/view-modes/compare.md) defaults to
**Text Diff** when both sides are text-shaped.

For a **row-level** comparison that survives column-order
differences:

1. Open CSV A → **View → Compare with…** → pick CSV B.
2. Switch the Compare view's sub-mode to **Row Hash Diff**.
3. Check the column-picker boxes to include the columns you want
   to match on (e.g. just `id`, or `customer_id` + `order_date`).
4. Three buckets appear: **Left-only**, **Right-only**, **Shared**.

Cross-format works the same way: compare CSV vs Parquet, or
SQLite vs JSON. The hash sees only `CellValue::to_string` output.

## Compare two database tables

Open both tables in two tabs (`File → Open Directory…` and the
[folder sidebar](../usage/tabs-and-sidebar.md#the-folder-sidebar)
make this fast). Then:

1. **Activate tab A**, then **View → Compare with…** and pick the
   file that holds tab B's table.
2. The
   [table picker](../getting-started/supported-formats.md#multi-table-files)
   fires; choose tab B's table.
3. Switch to **Row Hash Diff** and pick the primary-key column(s).

Same buckets, same semantics.

## Extract text from an EPUB

[EPUBs](../usage/view-modes/epub-reader.md) open in the Reader view
by default, but the [Table view](../usage/table-view.md) exposes
every paragraph as a row:

| chapter | paragraph | text                                   |
|---------|-----------|----------------------------------------|
| 1       | 1         | Cover                                  |
| 2       | 1         | The Lion, the Witch and the Wardrobe   |
| 2       | 2         | # The Lion, the Witch and the Wardrobe |
| ...     | ...       | ...                                    |

From there:

- [**Search**](../usage/search-and-filter.md) the toolbar for any
  word to find every paragraph containing it.
- [**SQL**](../usage/sql.md) (Ctrl+J) for more powerful
  queries:

    ```sql
    -- Count word frequency by chapter
    SELECT chapter, COUNT(*) AS paragraphs,
           SUM(LENGTH(text) - LENGTH(REPLACE(text, ' ', '')) + 1) AS approx_words
    FROM data
    GROUP BY chapter
    ORDER BY chapter;

    -- Find the longest paragraph
    SELECT chapter, paragraph, LENGTH(text) AS chars, text
    FROM data
    ORDER BY chars DESC
    LIMIT 5;

    -- Every occurrence of a name
    SELECT chapter, paragraph, text
    FROM data
    WHERE text LIKE '%Aslan%'
    ORDER BY chapter, paragraph;
    ```

## Pipe Octa into other tools

The [CLI](../cli/index.md)'s `-f json` output is `jq`-friendly:

```bash
# Extract email addresses
octa --sql users.parquet -q 'SELECT email FROM data WHERE active' -f json \
  | jq -r '.[].email'

# Count rows per group
octa --head sales.csv -n 1000 -f json \
  | jq 'group_by(.region) | map({region: .[0].region, n: length})'

# Sample 10 random rows
octa --sql data.csv -q 'SELECT * FROM data USING SAMPLE 10 ROWS' -f json \
  | jq '.[0]'
```

Octa writes data to stdout, status messages to stderr, so pipes
stay clean even on errors.

## Quickly inspect an unknown file

When someone hands you a file with no documentation:

```bash
# 1. Schema (cheap)
octa --schema mystery.dat

# 2. First few rows
octa --head mystery.dat -n 5

# 3. Row count
octa --sql mystery.dat -q 'SELECT count(*) FROM data'

# 4. Open in GUI for visual exploration
octa mystery.dat
```

See [`--schema`](../cli/schema.md), [`--head`](../cli/head.md),
and [`--sql`](../cli/sql.md) for the full reference on each step.

If the extension isn't recognised at all, Octa falls back to the
plain-text reader so step 1 still works; you'll get one column
called `Line` with one row per file line.

## Convert SQLite to multiple Parquet files

Octa's `--convert` writes the **first** table from a multi-table
source. To export every table:

```bash
# (Pseudo-script; adapt for your shell)
for table in users orders products; do
  octa --sql app.sqlite -q "SELECT * FROM ${table}" -f json > /tmp/dump.json
  octa --convert /tmp/dump.json "${table}.json"
done
rm /tmp/dump.json
```

Or use Octa's GUI: open the SQLite, the
[table picker](../getting-started/supported-formats.md#multi-table-files)
shows every table, load one, then **File → Save As…** as Parquet,
and repeat.

Or use DuckDB directly (which is what Octa uses under the hood):

```bash
duckdb -c "ATTACH 'app.sqlite' AS db (TYPE SQLITE);
           COPY db.users TO 'users.parquet';
           COPY db.orders TO 'orders.parquet';
           COPY db.products TO 'products.parquet';"
```

## Read-only mode for safety

Inspecting a production data file you can't risk modifying? Press
[**F8**](../reference/shortcuts.md#view) to enter
[read-only mode](../usage/editing.md#read-only-mode). Every editing
path is disabled (cell edits, structural changes, marks,
undo/redo), but you can still:

- [Search and filter](../usage/search-and-filter.md).
- Sort columns.
- Run [SQL queries](../usage/sql.md) (mutations don't
  persist anyway).
- Save As to a different file.

The status bar shows `[Read-only]` while active. Toggle off with
F8 again.

## See also

- [Search & Filter](../usage/search-and-filter.md) covers Plain,
  Wildcard, and Regex modes.
- [SQL panel](../usage/sql.md) is the full DuckDB surface
  inside the GUI.
- [`octa --sql`](../cli/sql.md) is the same surface from the shell.
- [Compare view](../usage/view-modes/compare.md) covers the Text
  Diff and Row Hash Diff sub-modes.
