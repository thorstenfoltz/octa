# Databases from the CLI

List, query, write, copy and export tables of a saved database connection
without opening the GUI. The same code runs behind the sidebar, the SQL panel
and the MCP tools, so all of them behave the same way.

Connections are the ones saved under **Settings > Databases** (see
[Database Connections](../usage/database-connections.md)). `--db` takes the
connection's name (case-insensitive) or its id. `octa --list-connections`
prints what is saved, and `--add-connection` / `--remove-connection` manage
them without the Settings dialog: see
[Managing connections](cloud.md#managing-connections).

Every connection is read-only until its **Allow writes** switch is on. That
switch gates `--db-write-table`, `--db-copy` (on the target) and any
mutating `--db-query`; reading, listing, exporting and `--sync-sql` never
write to the server. On a read-only connection `--db-query` only runs a
statement that starts with `SELECT`, `WITH`, `SHOW`, `DESCRIBE` or `EXPLAIN`,
and PostgreSQL, MySQL / MariaDB, ClickHouse and SQL Server hold the
connection read-only on the server too (see
[Writes](../usage/database-connections.md#writes)).

## Listing tables

```bash
octa --db warehouse --db-tables
```

Prints every schema and table the connection can see.

## Querying

```bash
octa --db warehouse --db-query "SELECT * FROM public.users LIMIT 10"
```

The query runs on the server in the engine's own SQL dialect, not in DuckDB.
Output works like every other action: `-f tsv|csv|json` and `--rows`.

To see the server's plan without running the statement, add `--explain`
(PostgreSQL, MySQL / MariaDB, Databricks and BigQuery; one statement). The
default is the steps in plain words, numbered in the order the server works
through them; `-f json` or `-f csv` gives one row per step. Nothing is
executed.

```bash
octa --db warehouse --db-query "SELECT * FROM public.users WHERE id > 10" --explain -f json
```

In the app: [Explain a query](../usage/sql.md#explain-a-query).

## Writing a file into a table

```bash
octa --db warehouse --db-write-table staging.users users.parquet --db-write-mode replace
```

`--db-write-mode` is `create` (the default: error if the table exists),
`replace` (drop and recreate) or `append` (insert into an existing table).

## Catalogs

Trino, Snowflake, Databricks and BigQuery have a catalog level above the
schema. Pass it with `--db-catalog`:

```bash
# list the catalogs
octa --db warehouse --db-tables

# list the tables inside one
octa --db warehouse --db-tables --db-catalog sales_prod

# write into a table in one
octa --db warehouse --db-write-table analytics.daily \
     --db-catalog sales_prod rows.parquet
```

Without `--db-catalog`, `--db-tables` on those engines lists the catalogs
rather than recursing into every schema of every catalog. On the other
engines `--db-catalog` is an error, because they have no catalog level.

## Copying a table between servers

```bash
octa --db source_conn --db-copy analytics.orders \
     --db-copy-to target_conn \
     --db-copy-target reporting.orders \
     --db-write-mode replace
```

The target table defaults to the source schema and table, so
`--db-copy-target` is optional. On an engine with catalogs, name them with
`--db-catalog` (source) and `--db-copy-target-catalog` (target). The target
connection's **Allow writes** switch must be on. PostgreSQL, MySQL/MariaDB
and Redshift copy directly server to server; the other engines stream through
Octa, exactly as the dialog's two lanes do (see
[Copying a table between servers](../usage/working-with-databases.md#copying-a-table-between-servers)).

## Exporting tables to files

```bash
# every table of the schema, as Parquet
octa --db warehouse --db-export sales --out-dir ./dump

# two tables as CSV, replacing files that are already there
octa --db warehouse --db-export sales --db-export-tables orders,customers \
     --out-dir ./dump --to csv --overwrite
```

`--to` takes `parquet` (the default), `csv`, `tsv` or `jsonl`. Without
`--db-export-tables` every table of the schema is exported; on an engine with
catalogs, name the catalog with `--db-catalog`. A file that is already there
is skipped unless `--overwrite`. Each table prints one line on stderr (rows
written, skipped or why it failed), and the exit code is 1 when any table
failed. CSV uses the write options from the settings file, like
[`--convert`](convert.md).

## Generating the sync SQL

`--sync-sql` answers a different question from `--db-query`: what SQL would
make this server table match this file? It reads the table, compares it on the
key columns you name, and prints one transaction. Nothing is written.

```bash
octa --sync-sql users.csv --db prod --sync-table public.users --sync-on id > change.sql
```

The script goes to stdout and the counts to stderr, so it pipes straight into a
file or into `psql`. This is the headless twin of the GUI's **File > Save SQL**,
and both call the same renderer, so a script reviewed here and a write-back
applied there cannot drift apart.

Numbers are compared as numbers, so a file's `120.50` and a `numeric(12,2)`
column's `120.50` do not produce a phantom UPDATE. Columns present only in the
file are reported and skipped; this never emits `ALTER TABLE`.

## Query timeout

Trino, Athena, Snowflake, Databricks and BigQuery give up on a query that makes
no progress after the connection's timeout (default 60 seconds). Set it with
`query_timeout=` in an `--add-connection` spec; see
[Query timeout](../usage/working-with-databases.md#query-timeout).

## See also

- In the app: [Database Connections](../usage/database-connections.md) and
  [Working with Databases](../usage/working-with-databases.md)
- MCP: [`list_db_connections`](../mcp/tools/list_db_connections.md),
  [`list_db_tables`](../mcp/tools/list_db_tables.md),
  [`query_db`](../mcp/tools/query_db.md),
  [`write_db_table`](../mcp/tools/write_db_table.md),
  [`copy_db_table`](../mcp/tools/copy_db_table.md),
  [`export_db_tables`](../mcp/tools/export_db_tables.md),
  [`sync_sql`](../mcp/tools/sync_sql.md)
- [Man page](man-page.md) for every flag
