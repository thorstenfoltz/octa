# `octa --sql`

Run a DuckDB SQL query against a file and print the result.
The file is loaded once, registered as a temp table called **`data`**,
and the query runs against it.

## Synopsis

```bash
octa --sql FILE -q '<query>' [-f tsv|json|csv]
```

| Flag                        | Required | Meaning                                                           |
|-----------------------------|----------|-------------------------------------------------------------------|
| `--sql FILE`                | yes      | The file to query.                                                |
| `-q QUERY`, `--query QUERY` | yes      | The SQL query string. Always reference the file's data as `data`. |
| `-f`, `--format`            | no       | Output format (default `tsv`).                                    |

## Examples

### Basic select

```bash
$ octa --sql sales.parquet -q 'SELECT region, SUM(amount) AS total FROM data GROUP BY region'
region   total
EU       4856732.50
US       9201443.75
APAC     3219008.00
```

### Filter + limit

```bash
octa --sql sales.csv -q 'SELECT * FROM data WHERE amount > 1000 LIMIT 10'
```

### Describe a file's schema

```bash
$ octa --sql data.parquet -q 'DESCRIBE data'
column_name  column_type  null  ...
region       VARCHAR      YES
quarter      VARCHAR      YES
amount       DOUBLE       YES
order_id     BIGINT       NO
```

### Top-N

```bash
$ octa --sql users.parquet -q '
  SELECT country, COUNT(*) AS n
  FROM data
  GROUP BY country
  ORDER BY n DESC
  LIMIT 5
'
country  n
US       483291
DE       119008
UK       104223
FR        87502
JP        76140
```

### Window functions (DuckDB's full SQL)

```bash
$ octa --sql events.parquet -q '
  SELECT user_id,
         event,
         timestamp,
         LAG(event) OVER (PARTITION BY user_id ORDER BY timestamp) AS prev_event
  FROM data
'
```

### JSON output for downstream tooling

```bash
$ octa --sql data.parquet -q 'SELECT * FROM data LIMIT 3' -f json
[
  { "id": 1, "name": "Alice", "active": true },
  { "id": 2, "name": "Bob", "active": false },
  { "id": 3, "name": "Charlie", "active": true }
]
```

Piping into `jq` or another tool works seamlessly:

```bash
octa --sql data.csv -q 'SELECT email FROM data WHERE active' -f json | jq -r '.[].email'
```

## Mutations

`INSERT` / `UPDATE` / `DELETE` are accepted but **do not persist
back to disk** (same as in the GUI if you don't save), since the in-memory
DuckDB connection is discarded at the end of the run. After a mutation,
the post-mutation contents of `data` are printed so you can pipe them through
[`--convert`](convert.md) if you want to persist:

```bash
# Filter rows and write the result to a new Parquet file
octa --sql in.csv -q 'SELECT * FROM data WHERE region = "EU"' -f csv > eu.csv
octa --convert eu.csv eu.parquet
```

A "rows affected" count is written to **stderr** for mutations:

```
2 rows affected
```

## How files become `data`

Octa's standard reader produces a `DataTable`. The CLI registers
that as a DuckDB temp table named **`data`** in a fresh in-memory
connection, then runs your query. The connection is **single-use**;
the next `octa --sql` invocation starts from scratch.

This is the same execution path as the GUI's SQL view, so:

- Column types match DuckDB's type system: `BIGINT`, `DOUBLE`,
  `VARCHAR`, `TIMESTAMP`, `BLOB`, etc.
- Quoted identifiers (`"some column"`) work for column names with
  spaces.

## Performance

- For streaming formats (Parquet, CSV, TSV), the initial-load row
  cap (5,000,000 rows by default) is applied before the query runs.
  `octa --sql huge.parquet -q 'SELECT count(*) FROM data'` counts
  the cap, not the full file. Override per-call with `--rows N|all`:

  ```bash
  # Raise the cap to 10 million for this query
  octa --sql huge.parquet -q 'SELECT count(*) FROM data' --rows 10,000,000

  # Read every row in the file
  octa --sql huge.parquet -q 'SELECT count(*) FROM data' --rows all
  ```

- Parquet files with very many row groups (> 32,767 — common with
  Spark / streaming ingest) fall back to a DuckDB-backed reader
  automatically, so they open without manual recompaction.
- DuckDB itself is highly optimised, so queries on millions of rows
  typically run in under a second.

## Notes

- The query is **case-insensitive on keywords** but **case-sensitive
  on identifiers** (column names). Use double quotes for names with
  unusual casing: `"MyColumn"`.
- Errors (parse errors, type mismatches) print to **stderr** with a
  DuckDB-formatted error message. The exit code is `1`.
- `EXPLAIN` / `EXPLAIN ANALYZE` work and print the DuckDB plan.

## See also

- [SQL panel](../usage/sql.md) is the same engine inside
  the GUI.
- [`octa --convert`](convert.md) writes the result to a different
  format.
- [MCP `run_sql` tool](../mcp/tools/run_sql.md) is the same query
  path via MCP.
