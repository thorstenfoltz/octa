# MCP Examples

Worked examples of using the Octa MCP server from an AI client.
Each one shows the user prompt, the tools the model would
typically call, and a sketch of the response.

These are illustrative; your real model might split calls
differently or ask follow-up questions. The point is to show the
rhythm of an MCP-backed conversation.

## Discovery: "What's in this file?"

> **User**: What's in `~/data/quarterly-sales.parquet`?

The fastest path is a single [`describe_file`](tools/describe_file.md)
call, which collapses `schema`, `count_rows`, and `read_table` into one
trip.

```json
// describe_file
{
  "path": "~/data/quarterly-sales.parquet",
  "sample_rows": 5
}
// →
{
  "path": "~/data/quarterly-sales.parquet",
  "format_name": "Parquet",
  "file_size_bytes": 1048576,
  "row_count": 47832,
  "initial_load_capped": false,
  "columns": [
    { "name": "region", "type": "Utf8" },
    { "name": "quarter", "type": "Utf8" },
    { "name": "product_id", "type": "Int64" },
    { "name": "amount", "type": "Float64" },
    { "name": "currency", "type": "Utf8" }
  ],
  "column_count": 5,
  "sample_rows": [
    ["EU", "Q1", 1001, 1234.50, "EUR"],
    ["US", "Q1", 1002, 9876.00, "USD"],
    …
  ],
  "sample_row_count": 5
}
```

**Model response**:

> The file is a 1 MB Parquet table with 47,832 rows and 5 columns:
> `region`, `quarter`, `product_id`, `amount`, `currency`. Looking
> at the first 5 rows, regions include EU and US; quarters Q1
> through Q4; amounts are in the low thousands. Want me to drill
> into any column?

The separate three calls (`schema` → `count_rows` → `read_table`)
are the right choice when you only want one of the
three pieces, e.g. just the schema:

```json
{ "path": "~/data/quarterly-sales.parquet" }
// → { "columns": [...], "column_count": 5 }
```

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

## Multi-table JOIN across formats

> **User**: Join `sales.parquet` against `customers.csv` and give
> me the top spenders.

A single [`run_sql`](tools/run_sql.md) call with `extra_tables`
registers the CSV alongside the Parquet primary file and runs the
JOIN in one shot:

```json
{
  "path": "~/data/sales.parquet",
  "extra_tables": [
    { "name": "customers", "path": "~/data/customers.csv" }
  ],
  "query": "SELECT c.name, SUM(s.amount) AS total FROM data s JOIN customers c ON s.cid = c.cid GROUP BY c.name ORDER BY total DESC LIMIT 5"
}
```

**Model response**:

> Top 5 by spend: Carol ($300), Bob ($200), Alice ($150), Dan
> ($120), Eve ($95). The breakdown matches all 4,832 transactions
> across the 3 named customers.

For multi-table sources (SQLite, DuckDB, Excel) prefer `attach`:

```json
{
  "path": "~/data/sales.parquet",
  "attach": [
    { "alias": "wh", "path": "~/data/warehouse.duckdb" }
  ],
  "query": "SELECT count(*) FROM data d JOIN wh.main.products p ON d.cid = p.cid"
}
```

After `ATTACH`, every inner table is reachable as
`alias.schema.tbl`.

## Persist a SQL result back to DuckDB

> **User**: Roll up `sales.parquet` by region and stash the result
> in our `analytics.duckdb` warehouse under `reports.q4_summary`.

Pass `write_to` to `run_sql`; the SELECT result is written to the
target instead of returned. The file is created if missing, and
`create_schema_if_missing` builds the `reports` schema on first
use.

```json
{
  "path": "~/data/sales.parquet",
  "query": "SELECT region, SUM(amount) AS total FROM data GROUP BY region",
  "write_to": {
    "path": "~/data/analytics.duckdb",
    "schema": "reports",
    "table": "q4_summary",
    "mode": "create",
    "create_schema_if_missing": true
  }
}
// →
{
  "kind": "write_back",
  "rows_written": 3,
  "created_schema": true,
  "target": "/home/u/data/analytics.duckdb | reports.q4_summary"
}
```

**Model response**:

> Wrote 3 rows to `analytics.duckdb | reports.q4_summary` (schema
> created). Next quarter, set `"mode": "replace"` to refresh the
> table or `"append"` to add to it.

SQLite targets work identically. SQLite has no schemas so omit
`schema` (or pass `null` / `"main"`):

```json
{
  "path": "~/data/users.csv",
  "query": "SELECT * FROM data WHERE active = 1",
  "write_to": {
    "path": "~/data/users.sqlite",
    "table": "active_users",
    "mode": "replace"
  }
}
```

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

One [`compare_schemas`](tools/compare_schemas.md) call gives a
structured answer:

```json
{
  "path_a": "~/data/users-2023.parquet",
  "path_b": "~/data/users-2024.parquet"
}
// →
{
  "identical": false,
  "common": [
    { "name": "id", "type": "Int64" },
    { "name": "email", "type": "Utf8" }
  ],
  "only_in_a": [],
  "only_in_b": [
    { "name": "signup_source", "type": "Utf8" }
  ],
  "type_mismatches": []
}
```

**Model response**:

> Almost: `users-2024.parquet` adds a `signup_source` column
> (Utf8) that's not in the 2023 file. Everything else matches:
> same names, same types.

## Schema validation in a pipeline

> **User**: Export the schema of `sales.parquet` and validate next
> month's file against it.

A two-step round trip. First, lock in the schema with
[`export_schema`](tools/export_schema.md):

```json
{
  "path": "~/data/sales.parquet",
  "target": "json-schema"
}
// → { "code": "{ \"$schema\": …, \"properties\": { … } }" }
```

The model can save that string to `~/data/sales.schema.json`. Later,
when a new file arrives, call
[`validate_against_schema`](tools/validate_against_schema.md):

```json
{
  "path": "~/data/sales-jan.parquet",
  "schema_path": "~/data/sales.schema.json"
}
// →
{
  "matches": true,
  "diff": {
    "identical": true,
    "common": [
      { "name": "id", "type": "Int64" },
      { "name": "region", "type": "Utf8" },
      …
    ],
    "only_in_a": [],
    "only_in_b": [],
    "type_mismatches": []
  },
  "unparsed_types": []
}
```

**Model response**:

> `sales-jan.parquet` matches the locked schema exactly, with the same
> columns and the same types. Safe to ingest.

If the new file added a column or changed a type, `matches` would
flip to `false` and `diff` would point to the exact deltas.

## Primary-key reconnaissance

> **User**: What columns in `events.parquet` could serve as a
> primary key?

Use [`unique_columns`](tools/unique_columns.md). Single columns are
the cheap default; pass `max_combo_size: 2` to also test pairs:

```json
{
  "path": "~/data/events.parquet",
  "max_combo_size": 2
}
// →
{
  "total_rows": 1000000,
  "single": [
    { "column": "event_id", "distinct_count": 1000000, "null_count": 0, "is_unique": true },
    { "column": "user_id", "distinct_count": 8732, "null_count": 0, "is_unique": false },
    { "column": "ts", "distinct_count": 999132, "null_count": 0, "is_unique": false }
  ],
  "combos": [
    { "columns": ["user_id", "ts"], "distinct_count": 1000000, "is_unique": true }
  ]
}
```

**Model response**:

> `event_id` is unique on its own, so that's the natural primary key.
> If you also need a "natural" composite key, `(user_id, ts)` is
> also unique across all 1M rows.

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
