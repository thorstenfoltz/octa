# MCP Examples

Worked examples of using the Octa MCP server from an AI client.
Each one shows the user prompt, the tools the model would
typically call, and a sketch of the response.

These are illustrative; your real model might split calls
differently or ask follow-up questions. The point is to show the
rhythm of an MCP-backed conversation.

## Discovery: "What's in this file?"

> **User**: What's in `~/data/quarterly-sales.parquet`?

Typical tool flow:

1. **`schema`**: what columns?
2. **`count_rows`**: how big?
3. *(optionally)* **`read_table`** with `limit: 10`: sample values.

```json
// schema
{ "path": "~/data/quarterly-sales.parquet" }
// →
{
  "columns": [
    { "name": "region", "type": "Utf8" },
    { "name": "quarter", "type": "Utf8" },
    { "name": "product_id", "type": "Int64" },
    { "name": "amount", "type": "Float64" },
    { "name": "currency", "type": "Utf8" }
  ],
  "column_count": 5
}

// count_rows
{ "path": "~/data/quarterly-sales.parquet" }
// →
{ "row_count": 47832, "initial_load_capped": false, "initial_load_cap": 1000000 }

// read_table (sample)
{ "path": "~/data/quarterly-sales.parquet", "limit": 10 }
// → first 10 rows
```

**Model response**:

> The file is a Parquet table with 47,832 rows and 5 columns:
> `region`, `quarter`, `product_id`, `amount`, `currency`. Looking
> at the first 10 rows, regions include EU, US, and APAC; quarters
> Q1 through Q4; amounts are in the low thousands. Want me to drill
> into any column?

## Multi-table database

> **User**: What tables are in `app.sqlite`?

1. **`list_tables`** to enumerate everything.

```json
{ "path": "~/data/app.sqlite" }
// →
{
  "tables": [
    { "name": "users", "row_count": 1247832, "columns": [...] },
    { "name": "orders", "row_count": 4891002, "columns": [...] },
    { "name": "products", "row_count": 12408, "columns": [...] }
  ]
}
```

**Model response**:

> Three tables: **users** (1.2M rows), **orders** (4.9M rows), and
> **products** (12.4k rows). Want details on any specific one?

## Aggregation

> **User**: Total sales per region last quarter from
> `quarterly-sales.parquet`.

The model chooses `run_sql` over `read_table` because we want
aggregation:

```json
{
  "path": "~/data/quarterly-sales.parquet",
  "query": "SELECT region, SUM(amount) AS total FROM data WHERE quarter = 'Q4' GROUP BY region ORDER BY total DESC"
}
// →
{
  "kind": "select",
  "result": {
    "schema": [
      { "name": "region", "type": "Utf8" },
      { "name": "total", "type": "Utf8" }
    ],
    "rows": [
      ["US", "9201443.75"],
      ["EU", "4856732.50"],
      ["APAC", "3219008.00"]
    ],
    "row_count": 3,
    "truncated": false,
    "total_rows_available": 3,
    "cell_truncated": false
  }
}
```

**Model response**:

> Q4 totals: US led with $9.2M, EU $4.86M, APAC $3.22M. Want me to
> break it down by product or quarter?

## Cross-format conversion

> **User**: Convert `messy.xlsx` to a clean Parquet file at
> `clean.parquet`.

```json
// convert
{ "input": "~/data/messy.xlsx", "output": "~/data/clean.parquet" }
// →
{
  "rows_written": 14523,
  "cols_written": 7,
  "output": "~/data/clean.parquet"
}
```

**Model response**:

> Wrote 14,523 rows × 7 columns to `~/data/clean.parquet`. The
> Parquet file is properly typed (numeric columns are real numbers,
> dates are dates) and ~80% smaller than the XLSX.

## Filtered export

> **User**: Export just the EU rows from `quarterly-sales.parquet`
> to a CSV.

Two calls: filter via `run_sql`, then `convert` the result. But
since the SQL session is per-call, the cleanest path is to write
the SQL output directly:

```json
// run_sql: get the filtered rows
{
  "path": "~/data/quarterly-sales.parquet",
  "query": "SELECT * FROM data WHERE region = 'EU'",
  "limit": 0
}
// → rows[]: 12482 EU rows returned
```

For very large filtered results, the model would likely:

1. Use `count_rows`-like preview via `SELECT count(*)` first.
2. If reasonable, fetch with `limit: 0`.
3. *(can't currently `convert` the SQL output directly; that's a
   v1 limitation.)* The user would manually save via the GUI or
   use the CLI: `octa --sql IN -q 'SELECT ...' -f csv > out.csv`.

**Model response**:

> 12,482 EU rows. Currently the MCP server's `convert` tool runs on
> the original file, so there's no way to convert a SQL result
> directly. If you want this on disk, run from the terminal:
>
> ```bash
> octa --sql ~/data/quarterly-sales.parquet -q "SELECT * FROM data WHERE region = 'EU'" -f csv > eu.csv
> ```

## Schema comparison

> **User**: Do `users-2023.parquet` and `users-2024.parquet` have
> the same schema?

```json
// schema on file A
{ "path": "~/data/users-2023.parquet" }
// → 8 columns

// schema on file B
{ "path": "~/data/users-2024.parquet" }
// → 9 columns
```

**Model response**:

> Almost: `users-2024.parquet` adds a `signup_source` column
> (Utf8) that's not in the 2023 file. Everything else matches:
> same names, same types.

## Big-file row count

> **User**: How many rows in `events.parquet`?

```json
{ "path": "~/data/events.parquet" }
// →
{ "row_count": 5000000, "initial_load_capped": true, "initial_load_cap": 5000000 }
```

The capped flag tells the model the count may be an underestimate.
`run_sql` runs against the same 5 Million-rows in-memory snapshot, so
`SELECT count(*) FROM data` returns the same capped value unless
also called with `unlimited: true`.

**Model response**:

> The file contains at least 5,000,000 rows, but that's Octa's
> initial-load cap. Want me to call `count_rows` again with
> `unlimited: true` so we get the true total? (That reads the
> entire file from disk.)

After confirmation, re-call with the cap lifted:

```json
{ "path": "~/data/events.parquet", "unlimited": true }
// →
{ "row_count": 47832104, "initial_load_capped": false, "initial_load_cap": 18446744073709551615 }
```

## EPUB text search

> **User**: Does this [EPUB](../usage/view-modes/epub-reader.md)
> mention "Aslan"?

```json
{
  "path": "~/books/lewis-lwwt.epub",
  "query": "SELECT chapter, paragraph, text FROM data WHERE text LIKE '%Aslan%' LIMIT 10"
}
```

**Model response**:

> The first occurrence of "Aslan" is in Chapter 7, paragraph 14.
> Across the book it appears 247 times; want me to characterise
> the distribution by chapter?

The EPUB reader exposes the book as a per-paragraph table, so
SQL works on it like any other tabular file.

## GeoJSON property query

> **User**: Which features in `cities.geojson` have a population
> over a million?

```json
{
  "path": "~/data/cities.geojson",
  "query": "SELECT name, population, __geometry FROM data WHERE population > 1000000 ORDER BY population DESC"
}
```

**Model response**:

> Five cities exceed 1M: New York (8.3M), Los Angeles (3.9M),
> Chicago (2.7M), Houston (2.3M), Phoenix (1.6M). Geometry column
> contains the WKT representation; want me to show them on a map?
> (It can't render directly, but the user can open the file in
> Octa's GUI and switch to the
> [Map view](../usage/view-modes/map.md).)

## Sanity-checking before bulk operations

> **User**: I want to convert all my `.csv` files in `~/data/` to
> Parquet. What types will each column become?

The model can't enumerate files directly (no filesystem-listing
tool exposed by Octa's MCP server), but it can `schema` each one
the user names:

```
User → "schema for sales.csv, users.csv, orders.csv"
Model → calls schema 3× in parallel → reports each
```

This is a common gotcha: the MCP server only exposes data
operations, not file-system browsing. The user supplies the paths;
Octa reads them.

## See also

- [Tools reference](tools/index.md) is the full input schema for
  each tool used above.
- [Limits & truncation](limits-and-truncation.md) explains what
  `truncated` / `cell_truncated` mean for downstream calls.
- [Troubleshooting](troubleshooting.md) covers what to do when
  these examples don't work as shown.
