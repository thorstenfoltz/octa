# `run_sql`

Run a DuckDB SQL query against one or more files. The primary file
is loaded and registered as a temp table called `data`; your query
runs against it.

Additional files can be registered as workspace tables via
`extra_tables` and whole DuckDB / SQLite databases can be `ATTACH`-ed
via `attach`, so the same call can JOIN across formats. The SELECT
result can also be written back to a DuckDB or SQLite file via
`write_to` instead of being returned.

## When to use

- Filtering ("rows where amount > 1000").
- Aggregations ("total amount per region", "median order size").
- Window functions, joins to other DuckDB-accessible sources, etc.
- Multi-source JOINs across formats (Parquet + CSV + SQLite, etc.).
- Persisting the result back into a DuckDB schema or a SQLite table.
- Most "find me X in this file" prompts.

For raw schema discovery, [`schema`](schema.md) is cheaper. For
"read every row" with no filtering,
[`read_table`](read_table.md) is cleaner, since the SQL overhead
isn't free.

## Input schema

| Parameter      | Type     | Required? | Default               | Description                                                                                                                                                               |
|----------------|----------|-----------|-----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `path`         | string   | yes       | (no default)          | Path to the primary file (registered as `data`).                                                                                                                          |
| `query`        | string   | yes       | (no default)          | SQL query. Always reference the primary file as `data`.                                                                                                                   |
| `limit`        | int      | no        | server default (1000) | Maximum rows to return in the response. `0` = unlimited.                                                                                                                  |
| `table`        | string   | no        | (no default)          | For multi-table sources, the specific table to expose as `data`.                                                                                                          |
| `unlimited`    | bool     | no        | `false`               | Lift the 5,000,000-row file-loader cap so the query sees every row in every loaded file. Combine with `limit: 0` for full output.                                         |
| `extra_tables` | object[] | no        | `[]`                  | Additional tables to register before the query. Each entry: `{ "name": "...", "path": "...", "table": null }`. `name` is sanitised to a valid SQL identifier.             |
| `attach`       | object[] | no        | `[]`                  | Databases to `ATTACH` throughout the call. Each entry: `{ "alias": "...", "path": "..." }`. `.duckdb` / `.ddb` selects DuckDB; everything else (`.sqlite`, `.db`) SQLite. |
| `write_to`     | object   | no        | (none)                | When set, the SELECT result is written to a DuckDB or SQLite file instead of being returned. See [Write-back](#write-back) below.                                         |

## Response shape

For **SELECT** queries:

```json
{
  "kind": "select",
  "result": {
    "schema": [{ "name": "<col>", "type": "Utf8" }, …],
    "rows": [[<v>, <v>, …], …],
    "row_count": <n>,
    "truncated": <bool>,
    "total_rows_available": <n>,
    "cell_truncated": <bool>
  }
}
```

For **mutation** queries (INSERT / UPDATE / DELETE):

```json
{
  "kind": "mutation",
  "affected": <n>,
  "result": {
    "schema": [...],
    "rows": [...],   // post-mutation contents of `data`
    ...
  }
}
```

A `CREATE TABLE` / `CREATE VIEW` that adds one table to the workspace is a
mutation whose `result` holds **that table's rows** rather than `data`, with
its declared column types, and the response carries `"created": "<name>"`.
The table is dropped from the workspace again after it is read, so a later
call cannot query it; `CREATE TABLE x AS SELECT ...` is therefore a way to
get a typed result set back, not a way to keep state between calls.

For calls with **`write_to`**:

```json
{
  "kind": "write_back",
  "rows_written": <n>,
  "created_schema": <bool>,
  "target": "/path/to/db.duckdb | schema.table"
}
```

!!! warning "Mutations don't persist"

    The in-memory DuckDB connection is created fresh per call and
    discarded at the end. **Mutations don't write back to the file**,
    and the mutated state isn't visible to any subsequent tool call:
    [`convert`](convert.md), [`read_table`](read_table.md), and
    follow-up `run_sql` calls all re-read the original file from disk.
    The post-mutation rows are returned only so you can inspect the
    effect of the query.

    Practical takeaway: treat `run_sql` as read-only. Use mutations
    only for "what would the result look like if I did this" probes.

## Example calls

### Filter rows

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/users.parquet",
    "query": "SELECT email, country FROM data WHERE active = true LIMIT 50"
  }
}
```

### Aggregation

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/sales.csv",
    "query": "SELECT region, SUM(amount) AS total FROM data GROUP BY region ORDER BY total DESC"
  }
}
```

### Count rows on a Parquet file

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/events-2024.parquet",
    "query": "SELECT count(*) FROM data"
  }
}
```

Note: `data` is the in-memory snapshot Octa loaded through its
streaming reader, which honours the initial-load cap. On files that exceed the cap,
`SELECT count(*) FROM data` returns the **loaded** count, not the
file count, the same limitation as [`count_rows`](count_rows.md).
Pass `unlimited: true` to lift the cap for this call so the query
operates on every row:

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/events-2024.parquet",
    "query": "SELECT count(*) FROM data",
    "unlimited": true
  }
}
```

Parquet files with very many row groups (> 32,767) fall back to a
DuckDB-backed reader automatically.

### Querying a specific table in a database

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "users",
    "query": "SELECT country, COUNT(*) AS n FROM data GROUP BY country ORDER BY n DESC LIMIT 10"
  }
}
```

### JOIN across formats with `extra_tables`

Register one or more extra files into the same DuckDB connection
that hosts `data`, then JOIN them in the query. The chosen `name`
is sanitised to a valid SQL identifier (lowercased, non-alphanumeric
characters replaced with `_`).

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/sales.parquet",
    "extra_tables": [
      { "name": "customers", "path": "/tmp/customers.csv" }
    ],
    "query": "SELECT c.name, SUM(s.amount) AS total FROM data s JOIN customers c ON s.cid = c.cid GROUP BY c.name ORDER BY total DESC"
  }
}
```

For multi-table sources (SQLite / DuckDB / Excel / ODS), set
`extra_tables[*].table` to pick the inner table, or use `attach`
instead so every inner table is reachable in one shot.

### ATTACH a database with `attach`

Each `attach` entry runs DuckDB's `ATTACH` against the target file.
Every table inside is queryable as `alias.schema.tbl` (DuckDB) or
`alias.tbl` (SQLite via the DuckDB sqlite extension when bundled).
For SQLite builds without the extension, the workspace falls back to
per-table loading under names like `alias__table`.

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/sales.parquet",
    "attach": [
      { "alias": "wh", "path": "/tmp/warehouse.duckdb" }
    ],
    "query": "SELECT count(*) FROM data d JOIN wh.main.products p ON d.cid = p.cid"
  }
}
```

### Write-back

`write_to` persists the SELECT result to a DuckDB or SQLite file
instead of returning rows. The target file is created if missing.

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/sales.parquet",
    "query": "SELECT region, SUM(amount) AS total FROM data GROUP BY region",
    "write_to": {
      "path": "/tmp/analytics.duckdb",
      "schema": "reports",
      "table": "q4_summary",
      "mode": "create",
      "create_schema_if_missing": true
    }
  }
}
```

Fields:

| Field                      | Type   | Required? | Default  | Meaning                                                                                                         |
|----------------------------|--------|-----------|----------|-----------------------------------------------------------------------------------------------------------------|
| `path`                     | string | yes       | -        | Target DuckDB or SQLite file. Extension picks the kind (`.duckdb` / `.ddb` → DuckDB; everything else → SQLite). |
| `table`                    | string | yes       | -        | Target table name.                                                                                              |
| `schema`                   | string | no        | `null`   | Target schema (DuckDB only). `null` writes to `main`. SQLite has no schemas; pass `null` or `"main"`.           |
| `mode`                     | string | no        | `create` | `create` (errors if table exists), `replace` (drop + recreate), or `append` (`INSERT` into existing).           |
| `create_schema_if_missing` | bool   | no        | `false`  | When `true` and the target schema doesn't exist, create it (DuckDB only; ignored for SQLite).                   |

SQLite writes go through `rusqlite` directly, so they work even on
builds where the DuckDB sqlite extension isn't bundled.

### DESCRIBE / EXPLAIN

DuckDB's `DESCRIBE` is useful for schema discovery from inside SQL:

```json
{
  "name": "run_sql",
  "arguments": {
    "path": "/tmp/messy.parquet",
    "query": "DESCRIBE data"
  }
}
```

## What's available

The full DuckDB SQL surface (as exposed by the bundled DuckDB
library):

- All standard SQL features (SELECT / FROM / WHERE / GROUP BY /
  HAVING / ORDER BY / LIMIT).
- Common Table Expressions (CTEs / `WITH … AS`).
- Window functions (`ROW_NUMBER()`, `RANK()`, `LAG()`, percentiles).
- JSON functions (`json_extract`, `unnest`, …).
- Date / time / string functions.
- Regex functions.
- PIVOT / UNPIVOT.
- DESCRIBE / EXPLAIN.

## Notes

- Identifiers with spaces or special characters need to be
  double-quoted: `"My Column"`. Octa's column registration uses
  this convention to keep weird names round-tripping cleanly.
- Errors come back as `invalid_params` with a DuckDB-formatted
  message.
- A query that returns zero rows is reported with `row_count: 0`
  and an empty `rows` array, not an error.

## See also

- [Limits & truncation](../limits-and-truncation.md) covers
  `truncated` / `total_rows_available` semantics.
- [`read_table`](read_table.md) is for when you want everything,
  no filtering.
- [Examples](../examples.md) has SQL-heavy worked prompts.
