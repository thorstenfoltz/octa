# `query_db`

Run one SQL statement on a saved live-database connection, server-side.

The SQL is in **the engine's own dialect**, not DuckDB's: PostgreSQL,
MySQL/MariaDB, T-SQL, Snowflake SQL and so on. Nothing is rewritten on
the way. That is the difference from [`run_sql`](run_sql.md), which
loads files into an embedded DuckDB and speaks DuckDB SQL.

## Input schema

| Name         | Type              | Meaning                                                                   |
|--------------|-------------------|---------------------------------------------------------------------------|
| `connection` | string (required) | Saved connection name or id                                               |
| `sql`        | string (required) | One statement, in the server's native dialect                             |
| `limit`      | integer           | Response row cap for a SELECT. Absent: the server default. `0`: unlimited |
| `explain`    | boolean           | Return the query plan instead of running it (see [Explain](#explain))     |

`connection` may also name a saved DuckDB or SQLite file, which runs the SQL
in that file's own dialect. `explain` works on DuckDB, not on SQLite.

## Result

A SELECT returns the same shape as [`read_table`](read_table.md):

```json
{
  "schema": [{ "name": "id", "type": "Int64" }, { "name": "email", "type": "Utf8" }],
  "rows": [[1, "ada@example.com"]],
  "row_count": 1,
  "truncated": false
}
```

A mutation returns how much it changed:

```json
{ "rows_affected": 3 }
```

## Explain

Pass `explain: true` to get the server's plan instead of running the
statement (DuckDB, PostgreSQL, MySQL / MariaDB, Databricks and BigQuery; one statement; never
`EXPLAIN ANALYZE`). The response is:

```json
{
  "kind": "explain",
  "steps": [
    { "depth": 0, "step": "read_table", "label": "Read whole table", "table": "orders",
      "estimated_rows": 1200.0, "detail": "Filter: (amount > 1)", "server_name": "Seq Scan" }
  ],
  "plain": "1. Read whole table: orders, ~1,200 rows\n",
  "server_text": "the plan as the server printed it"
}
```

The step ids and fields are those of [`run_sql`](run_sql.md#explain).

`bytes_scanned` appears only on BigQuery (the bytes the query would read,
never a price). Nothing is executed, so the write gates below do not apply.

## Writes are gated twice

An `INSERT`, `UPDATE`, `DELETE` or DDL statement is refused unless the
connection's **Allow writes** switch is on
([`list_db_connections`](list_db_connections.md) reports it), and refused
again whenever the server was started with `--mcp-read-only`: that flag
blocks mutations here even on a connection that allows them.

Both gates only let through statements that start with `SELECT`, `WITH`,
`SHOW`, `DESCRIBE` or `EXPLAIN`, and on MySQL / MariaDB and BigQuery none with
a semicolon before its end, even one inside text or a comment: those servers
run every statement they are sent, so `SELECT 1; DELETE FROM t` would
otherwise delete. PostgreSQL, MySQL / MariaDB, ClickHouse and SQL Server also
hold such a connection read-only on the server, and `--mcp-read-only` makes
every connection read-only there in the same way. The full rules, SQL Server's
included, are under [Writes](../../usage/database-connections.md#writes).

## Add your own `LIMIT`

`limit` caps the **response**, not the query: the whole result set is
fetched from the server first. On a large table, put a `LIMIT` (or `TOP`
on SQL Server) in the SQL itself.

## See also

- [`list_db_tables`](list_db_tables.md) — find a table to query.
- [`sync_sql`](sync_sql.md) — generate the SQL for a change without
  running it.
- [Database Connections](../../usage/database-connections.md).
- In the app: [Working with Databases](../../usage/working-with-databases.md)
- Command line: [`--db-query`](../../cli/databases.md#querying)
- In the app: [Explain a query](../../usage/sql.md#explain-a-query)
