# `query_db`

Run one SQL statement on a saved live-database connection, server-side.

The SQL is in **the engine's own dialect**, not DuckDB's: PostgreSQL,
MySQL/MariaDB, T-SQL, Snowflake SQL and so on. Nothing is rewritten on
the way. That is the difference from [`run_sql`](run_sql.md), which
loads files into an embedded DuckDB and speaks DuckDB SQL.

## Parameters

| Name         | Type              | Meaning                                                                   |
|--------------|-------------------|---------------------------------------------------------------------------|
| `connection` | string (required) | Saved connection name or id                                               |
| `sql`        | string (required) | One statement, in the server's native dialect                             |
| `limit`      | integer           | Response row cap for a SELECT. Absent: the server default. `0`: unlimited |

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

## Writes are gated twice

An `INSERT`, `UPDATE`, `DELETE` or DDL statement is refused unless the
connection's **Allow writes** switch is on
([`list_db_connections`](list_db_connections.md) reports it), and refused
again whenever the server was started with `--mcp-read-only` — that flag
blocks mutations here even on a connection that allows them.

## Add your own `LIMIT`

`limit` caps the **response**, not the query: the whole result set is
fetched from the server first. On a large table, put a `LIMIT` (or `TOP`
on SQL Server) in the SQL itself.

## See also

- [`list_db_tables`](list_db_tables.md) — find a table to query.
- [`sync_sql`](sync_sql.md) — generate the SQL for a change without
  running it.
- [Database Connections](../../usage/database-connections.md).
