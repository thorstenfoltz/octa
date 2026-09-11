# `octa --sql`

Run a DuckDB SQL query against a file and print the result.
The file is loaded once, registered as a temp table called **`data`**,
and the query runs against it.

Extra tables can be loaded into the same query with `--sql-table` and
whole DuckDB / SQLite databases can be `ATTACH`-ed with `--sql-attach`,
so the same invocation can JOIN across formats. The SELECT result can
also be written back to a DuckDB or SQLite file via `--sql-write-to`.

## Synopsis

```bash
octa --sql FILE -q '<query>' [-f tsv|json|csv]
     [--sql-table NAME=PATH ...]
     [--sql-attach ALIAS=PATH ...]
     [--sql-write-to PATH --sql-write-table TABLE
        [--sql-write-schema SCHEMA] [--sql-write-mode create|append|replace]]
```

| Flag                        | Required              | Meaning                                                                                                                                                     |
|-----------------------------|-----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `--sql FILE`                | yes                   | The primary file to query, registered as `data`.                                                                                                            |
| `-q QUERY`, `--query QUERY` | yes                   | The SQL query string. Always reference the primary file as `data`.                                                                                          |
| `-f`, `--format`            | no                    | Output format (default `tsv`). Ignored when `--sql-write-to` is set.                                                                                        |
| `--sql-table NAME=PATH`     | no                    | Register an extra table from any supported file under the SQL name `NAME`. Repeatable.                                                                      |
| `--sql-attach ALIAS=PATH`   | no                    | `ATTACH` a DuckDB or SQLite database under `ALIAS`. Repeatable. Every inner table becomes queryable as `ALIAS.schema.tbl` (DuckDB) or `ALIAS.tbl` (SQLite). |
| `--sql-write-to PATH`       | no                    | Persist the SELECT result to this DuckDB or SQLite file. Created if missing.                                                                                |
| `--sql-write-table TABLE`   | with `--sql-write-to` | Target table name.                                                                                                                                          |
| `--sql-write-schema SCHEMA` | no                    | Target schema (DuckDB only; defaults to `main`). Pass for SQLite only as `main` or leave unset.                                                             |
| `--sql-write-mode MODE`     | no                    | `create` (default; errors if the table exists), `replace` (drop + recreate), or `append` (`INSERT` into existing).                                          |

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

## Multi-table JOIN

`--sql-table NAME=PATH` registers an extra file into the same DuckDB
connection that hosts `data`. The file is loaded via the standard
format registry, so any supported format works (CSV, Parquet, JSON,
Excel, SQLite, ...). The flag is repeatable; each pair gets its own
entry in the workspace.

```bash
$ octa --sql sales.parquet \
       --sql-table customers=customers.csv \
       -q '
  SELECT c.name, SUM(d.amount) AS total
  FROM data d
  JOIN customers c ON d.cid = c.cid
  GROUP BY c.name
  ORDER BY total DESC
'
name      total
Carol     300
Bob       200
Alice     150
```

Several `--sql-table` flags can be combined:

```bash
octa --sql sales.parquet \
     --sql-table customers=customers.csv \
     --sql-table regions=regions.json \
     -q '
  SELECT r.region_name, c.name, SUM(d.amount) AS total
  FROM data d
  JOIN customers c ON d.cid = c.cid
  JOIN regions   r ON d.region_id = r.id
  GROUP BY r.region_name, c.name
'
```

The chosen `NAME` is sanitised to a valid SQL identifier (lowercased,
non-alphanumeric characters replaced with `_`). For multi-table sources
prefer `--sql-attach` so every inner table is reachable.

## Attaching whole databases

`--sql-attach ALIAS=PATH` runs DuckDB's `ATTACH` against the target
file. Every table inside is queryable as `ALIAS.schema.tbl` (DuckDB)
or `ALIAS.tbl` (SQLite via the DuckDB sqlite extension when bundled;
otherwise the workspace falls back to per-table loading under names
like `ALIAS__table`).

```bash
$ octa --sql sales.parquet \
       --sql-attach wh=warehouse.duckdb \
       -q '
  SELECT p.name, SUM(d.amount) AS total
  FROM data d
  JOIN wh.main.products p ON d.cid = p.cid
  GROUP BY p.name
'
```

The extension `.duckdb` / `.ddb` selects DuckDB; everything else
(`.sqlite`, `.db`, `.sqlite3`) selects SQLite.

## Write-back to DuckDB / SQLite

`--sql-write-to PATH` persists the SELECT result instead of printing
it. Pair it with `--sql-write-table`, optionally `--sql-write-schema`
(DuckDB only), and `--sql-write-mode`. The target file is created if
missing.

```bash
# Create a fresh table in a new schema.
$ octa --sql sales.parquet -q '
  SELECT region, SUM(amount) AS total
  FROM data
  GROUP BY region
  ORDER BY region
' --sql-write-to analytics.duckdb \
  --sql-write-schema reports \
  --sql-write-table q4_summary
wrote 2 row(s) to analytics.duckdb | reports.q4_summary
```

Write modes:

| Mode      | Behaviour                                                                            |
|-----------|--------------------------------------------------------------------------------------|
| `create`  | Default. Errors if the target table already exists.                                  |
| `replace` | `DROP TABLE IF EXISTS` followed by `CREATE TABLE`.                                   |
| `append`  | `INSERT INTO` an existing table. Column count and order must match the SELECT shape. |

SQLite has no schemas, so `--sql-write-schema` must be omitted (or
explicitly `main`). The write goes through `rusqlite` directly, so it
works even on builds where the DuckDB sqlite extension isn't bundled.

```bash
# Append into an existing SQLite table.
octa --sql data.csv -q 'SELECT * FROM data WHERE active=1' \
     --sql-write-to users.sqlite \
     --sql-write-table active_users \
     --sql-write-mode append
```

The success line is printed to **stdout**:

```
wrote N row(s) to <path> | <schema>.<table>
```

For SQLite the `<schema>.` prefix is omitted. On success, nothing else
is written to stdout, so CI pipelines can compare against an expected
string. Errors print to stderr with a non-zero exit code.

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

`CREATE TABLE` / `CREATE VIEW` is the one mutation that prints something
else: the **created table's rows** (with its declared column types) go to
stdout and `created <name>` to stderr, so
`octa --sql in.csv -q 'CREATE TABLE top AS SELECT * FROM data LIMIT 10' -f parquet > top.parquet`
writes a typed extract in one step. The table is dropped from the connection
again after it is printed.

## How files become `data`

Octa's standard reader produces a `DataTable`. The CLI registers
that as a DuckDB temp table named **`data`** in a fresh in-memory
connection, then runs your query. Extra `--sql-table` entries land
as additional temp tables in the same connection, and `--sql-attach`
runs `ATTACH` on it, so JOINs across all of them are plain DuckDB
work. The connection is **single-use**; the next `octa --sql`
invocation starts from scratch.

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

- Parquet files with very many row groups (> 32,767, common with
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

## Files larger than memory

`--stream` registers the file as a DuckDB **view** instead of loading its
rows:

```bash
octa --sql huge.parquet --query 'SELECT count(*) FROM data' --stream
```

`data` then scans the file where it lies, so an aggregate covers every
row no matter what `--rows` says. This works for Parquet, CSV, TSV and
JSON; any other format falls back to a normal read with a note on
stderr, and so does every action other than `--sql`.

The trade is that nothing is held in memory to come back to: each query
re-reads what it needs. For repeated queries over the same large file,
convert it to Parquet once and query that.

See [Large Files](../usage/large-files.md).
