# `run_sql`

Run a DuckDB SQL query against a file. The file is loaded and
registered as a temp table called `data`; your query runs
against it.

## When to use

- Filtering ("rows where amount > 1000").
- Aggregations ("total amount per region", "median order size").
- Window functions, joins to other DuckDB-accessible sources, etc.
- Most "find me X in this file" prompts.

For raw schema discovery, [`schema`](schema.md) is cheaper. For
"read every row" with no filtering,
[`read_table`](read_table.md) is cleaner, since the SQL overhead
isn't free.

## Input schema

| Parameter   | Type   | Required? | Default               | Description                                                                                                             |
|-------------|--------|-----------|-----------------------|-------------------------------------------------------------------------------------------------------------------------|
| `path`      | string | yes       | (no default)          | Path to the file                                                                                                        |
| `query`     | string | yes       | (no default)          | SQL query. Always reference the file's data as `data`                                                                   |
| `limit`     | int    | no        | server default (1000) | Maximum rows to return in the response. `0` = unlimited                                                                 |
| `table`     | string | no        | (no default)          | For multi-table sources, the specific table to expose as `data`                                                         |
| `unlimited` | bool   | no        | `false`               | Lift the 5,000,000-row file-loader cap so the query sees every row in the file. Combine with `limit: 0` for full output |

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
